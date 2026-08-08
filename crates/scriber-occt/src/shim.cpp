#include "scriber-occt/src/shim.hpp"

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepGProp.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <STEPControl_StepModelType.hxx>
#include <STEPControl_Writer.hxx>
#include <StlAPI_Writer.hxx>

#include <string>

namespace scriber {

namespace {

// Symbolic name for the status, so the message does not depend on an integer
// whose meaning a reader would have to go look up.
const char *status_name(IFSelect_ReturnStatus status) {
  switch (status) {
    case IFSelect_RetVoid:
      return "IFSelect_RetVoid";
    case IFSelect_RetDone:
      return "IFSelect_RetDone";
    case IFSelect_RetError:
      return "IFSelect_RetError";
    case IFSelect_RetFail:
      return "IFSelect_RetFail";
    case IFSelect_RetStop:
      return "IFSelect_RetStop";
  }

  return "IFSelect_Ret<unknown>";
}

}  // namespace

std::unique_ptr<Shape> make_box(double dx, double dy, double dz) {
  return guard([&] {
    silence_kernel_console();
    BRepPrimAPI_MakeBox builder(dx, dy, dz);
    return std::make_unique<Shape>(Shape{builder.Shape()});
  });
}

std::unique_ptr<Shape> make_cylinder(double radius, double height) {
  return guard([&] {
    silence_kernel_console();
    BRepPrimAPI_MakeCylinder builder(radius, height);
    return std::make_unique<Shape>(Shape{builder.Shape()});
  });
}

std::unique_ptr<Shape> cut(const Shape &target, const Shape &tool) {
  return guard([&] {
    silence_kernel_console();
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
    silence_kernel_console();
    GProp_GProps props;
    BRepGProp::VolumeProperties(shape.inner, props);
    return props.Mass();
  });
}

void write_step(const Shape &shape, rust::Str path) {
  guard([&] {
    silence_kernel_console();

    STEPControl_Writer writer;

    // OCCT reports the useful detail to its messenger, not through the
    // exception, so fold the status code into the message we throw. The path
    // is deliberately excluded: Rust already knows it, and a later task would
    // otherwise print it twice.
    const IFSelect_ReturnStatus transferred =
        writer.Transfer(shape.inner, STEPControl_AsIs);
    if (transferred != IFSelect_RetDone) {
      throw std::runtime_error(std::string("STEP transfer failed (") +
                               status_name(transferred) + ")");
    }

    // rust::Str is not null-terminated, so copy before handing to OCCT.
    const std::string target(path.data(), path.size());

    const IFSelect_ReturnStatus written = writer.Write(target.c_str());
    if (written != IFSelect_RetDone) {
      throw std::runtime_error(std::string("STEP write failed (") +
                               status_name(written) + ")");
    }
  });
}

void write_stl(const Shape &shape, rust::Str path) {
  guard([&] {
    silence_kernel_console();

    // STL is a mesh format. OCCT does not triangulate on demand, so an
    // unmeshed shape writes a valid-looking file with zero facets.
    BRepMesh_IncrementalMesh mesher(shape.inner, 0.01);
    if (!mesher.IsDone()) {
      throw std::runtime_error("STL meshing failed");
    }

    const std::string target(path.data(), path.size());

    StlAPI_Writer writer;
    if (!writer.Write(shape.inner, target.c_str())) {
      throw std::runtime_error("STL write failed");
    }
  });
}

}  // namespace scriber
