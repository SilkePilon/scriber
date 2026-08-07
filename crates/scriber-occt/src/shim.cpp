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
    // The constructor already runs the operation. Calling Build() again would
    // clear and re-run the whole DS filler, doubling the cost of every cut.
    BRepAlgoAPI_Cut op(target.inner, tool.inner);

    if (!op.IsDone()) {
      throw std::runtime_error("boolean cut failed");
    }

    // IsDone() only means the algorithm ran; subtracting a larger solid
    // succeeds and yields an empty compound. Detecting that is the kernel
    // layer's job (Error::EmptyResult), not the bridge's.
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
