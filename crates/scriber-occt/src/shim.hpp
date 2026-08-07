#pragma once

#include <memory>
#include <stdexcept>
#include <string>
#include <utility>

#include <Standard_Failure.hxx>
#include <Standard_Type.hxx>
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
    // Many OCCT failures carry an empty message, so lead with the exception
    // class name (Standard_DomainError, StdFail_NotDone, ...) which is always
    // present and is usually the more diagnostic half.
    const Standard_CString kind = failure.DynamicType()->Name();
    std::string text = kind != nullptr ? kind : "Standard_Failure";

    const Standard_CString message = failure.GetMessageString();
    if (message != nullptr && *message != '\0') {
      text += ": ";
      text += message;
    }

    throw std::runtime_error(text);
  }
}

std::unique_ptr<Shape> make_box(double dx, double dy, double dz);

double volume(const Shape &shape);

}  // namespace scriber
