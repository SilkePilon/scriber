#pragma once

#include <memory>
#include <stdexcept>
#include <string>
#include <utility>

#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <Standard_Failure.hxx>
// Defines OCC_VERSION_MAJOR, used below to pick the right accessors for the
// exception's class name and message.
#include <Standard_Version.hxx>
// Still required: STANDARD_TYPE() in silence_kernel_console() below is defined
// here, and DynamicType()->Name() on the 7.x failure path also needs it.
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
    // Both accessors -- class name and message -- moved between OCCT
    // generations, and we develop on 7.9.3 while shipping 8.0.1, so both
    // branches have to compile. 7.x carries RTTI from its Standard_Transient
    // base and spells the message GetMessageString(); 8.0 dropped that base
    // (Standard_Failure now derives from std::exception) and exposes
    // ExceptionType() for the class name plus what() for the message, leaving
    // GetMessageString() behind as a deprecated alias that warns. Print() does
    // exist in both, but it prefixes a raw pointer address, which has no place
    // in a message a user reads.
#if OCC_VERSION_MAJOR >= 8
    const char *kind = failure.ExceptionType();
    const char *message = failure.what();
#else
    const char *kind = failure.DynamicType()->Name();
    const char *message = failure.GetMessageString();
#endif

    std::string text =
        (kind != nullptr && *kind != '\0') ? kind : "Standard_Failure";

    if (message != nullptr && *message != '\0') {
      text += ": ";
      text += message;
    }

    throw std::runtime_error(text);
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

void write_stl(const Shape &shape, rust::Str path);

}  // namespace scriber
