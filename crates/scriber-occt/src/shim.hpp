#pragma once

#include <memory>
#include <stdexcept>
#include <utility>

#include <Standard_Failure.hxx>
#include <TopoDS_Shape.hxx>

#include "rust/cxx.h"

namespace scriber {

// A single opaque wrapper so cxx only has to know about one C++ type.
// TopoDS_Shape is a handle-like value type in OCCT, so copying it is cheap.
struct Shape {
  TopoDS_Shape inner;
};

// OCCT signals failure by raising Standard_Failure, which derives from
// Standard_Transient and NOT from std::exception. cxx's generated catch
// handler only looks for std::exception, so an untranslated OCCT failure
// would unwind straight through an extern "C" frame and abort the process.
//
// Every shim function that calls into OCCT must route through this guard so
// the failure arrives in Rust as an Err instead of terminating.
template <typename Body>
auto guard(Body &&body) -> decltype(body()) {
  try {
    return std::forward<Body>(body)();
  } catch (const Standard_Failure &failure) {
    const Standard_CString message = failure.GetMessageString();
    throw std::runtime_error(message != nullptr && *message != '\0'
                                 ? message
                                 : "OpenCASCADE operation failed");
  }
}

std::unique_ptr<Shape> make_box(double dx, double dy, double dz);

double volume(const Shape &shape);

}  // namespace scriber
