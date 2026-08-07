#pragma once

#include <memory>

#include <TopoDS_Shape.hxx>

#include "rust/cxx.h"

namespace scriber {

// A single opaque wrapper so cxx only has to know about one C++ type.
// TopoDS_Shape is a handle-like value type in OCCT, so copying it is cheap.
struct Shape {
  TopoDS_Shape inner;
};

std::unique_ptr<Shape> make_box(double dx, double dy, double dz);

double volume(const Shape &shape);

}  // namespace scriber
