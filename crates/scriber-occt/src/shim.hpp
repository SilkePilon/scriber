#pragma once

#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>

#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <Standard_Failure.hxx>
// Still required: STANDARD_TYPE() in silence_kernel_console() below is defined
// here. It is no longer needed for the failure path, which used to call
// DynamicType()->Name().
#include <Standard_Type.hxx>
#include <TopoDS_Shape.hxx>

#include "rust/cxx.h"

namespace scriber {

// A single opaque wrapper so cxx only has to know about one C++ type.
// TopoDS_Shape is a handle-like value type in OCCT, so copying it is cheap.
struct Shape {
  TopoDS_Shape inner;
};

// Drops OCCT's console printers the first time any shim function runs, so
// kernel diagnostics never land on stdout. The function-local static makes
// this thread-safe and once-only under C++11 and later.
inline void silence_kernel_console() {
  static const bool done = [] {
    Message::DefaultMessenger()->RemovePrinters(
        STANDARD_TYPE(Message_PrinterOStream));
    return true;
  }();
  (void)done;
}

// OCCT signals failure by raising Standard_Failure. On 7.x it derives from
// Standard_Transient and NOT from std::exception, so cxx's generated handler
// -- which only looks for std::exception -- would let it unwind straight
// through an extern "C" frame and abort the process. Every shim function that
// calls into OCCT must therefore route through this guard, so the failure
// arrives in Rust as an Err instead of terminating.
//
// Clause order below is load-bearing and must not be rearranged: on 8.x
// Standard_Failure IS a std::exception, so if the generic rethrow clause came
// first it would swallow every OCCT failure and drop its class name from the
// message.
template <typename Body>
auto guard(Body &&body) -> decltype(body()) {
  try {
    return std::forward<Body>(body)();
  } catch (const Standard_Failure &failure) {
    // Print() writes "ExceptionClass: message" and is the only accessor that
    // exists in both OCCT 7.x and 8.x. Do not reach for the alternatives:
    // DynamicType() exists only in 7.x (8.0 dropped Standard_Transient as the
    // base and derives Standard_Failure from std::exception instead), and
    // ExceptionType() exists only in 8.x. We develop on 7.9.3 and ship 8.0.1,
    // so anything version-specific compiles here and breaks in the Flatpak.
    std::ostringstream stream;
    failure.Print(stream);

    const std::string text = stream.str();
    throw std::runtime_error(text.empty() ? "OpenCASCADE operation failed"
                                          : text);
  } catch (const std::exception &) {
    // Already the shape cxx expects (including the shim's own throws, such as
    // the IsDone() check in cut), so let it through with its message intact
    // rather than flattening it into the catch-all below.
    throw;
  } catch (...) {
    // OCCT's hierarchy is rooted at Standard_Failure and cxx already handles
    // std::exception, so reaching here should be impossible. Catching anyway
    // costs nothing and keeps a stray throw from aborting the process.
    throw std::runtime_error("unknown C++ exception from OpenCASCADE");
  }
}

std::unique_ptr<Shape> make_box(double dx, double dy, double dz);

std::unique_ptr<Shape> make_cylinder(double radius, double height);

std::unique_ptr<Shape> cut(const Shape &target, const Shape &tool);

double volume(const Shape &shape);

void write_step(const Shape &shape, rust::Str path);

}  // namespace scriber
