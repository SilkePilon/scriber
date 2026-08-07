#include "scriber-occt/src/shim.hpp"

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>

namespace scriber {

std::unique_ptr<Shape> make_box(double dx, double dy, double dz) {
  return guard([&] {
    BRepPrimAPI_MakeBox builder(dx, dy, dz);
    return std::make_unique<Shape>(Shape{builder.Shape()});
  });
}

std::unique_ptr<Shape> make_cylinder(double radius, double height) {
  return guard([&] {
    BRepPrimAPI_MakeCylinder builder(radius, height);
    return std::make_unique<Shape>(Shape{builder.Shape()});
  });
}

std::unique_ptr<Shape> cut(const Shape &target, const Shape &tool) {
  return guard([&] {
    BRepAlgoAPI_Cut op(target.inner, tool.inner);
    op.Build();

    if (!op.IsDone()) {
      throw std::runtime_error("boolean cut failed");
    }

    return std::make_unique<Shape>(Shape{op.Shape()});
  });
}

double volume(const Shape &shape) {
  return guard([&] {
    GProp_GProps props;
    BRepGProp::VolumeProperties(shape.inner, props);
    return props.Mass();
  });
}

}  // namespace scriber
