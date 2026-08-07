#include "scriber-occt/src/shim.hpp"

#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <GProp_GProps.hxx>

namespace scriber {

std::unique_ptr<Shape> make_box(double dx, double dy, double dz) {
  BRepPrimAPI_MakeBox builder(dx, dy, dz);
  builder.Build();
  return std::make_unique<Shape>(Shape{builder.Shape()});
}

double volume(const Shape &shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.inner, props);
  return props.Mass();
}

}  // namespace scriber
