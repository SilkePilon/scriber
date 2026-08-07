#include "scriber-occt/src/shim.hpp"

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <STEPControl_StepModelType.hxx>
#include <STEPControl_Writer.hxx>

#include <string>

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

namespace {

// OCCT's default messenger prints a "Statistics on Transfer" banner to stdout
// on every write. A CAD application owns its own stdout, so drop the console
// printers once, the first time we touch the data-exchange layer.
void silence_kernel_console() {
  static const bool done = [] {
    Message::DefaultMessenger()->RemovePrinters(
        STANDARD_TYPE(Message_PrinterOStream));
    return true;
  }();
  (void)done;
}

}  // namespace

void write_step(const Shape &shape, rust::Str path) {
  guard([&] {
    silence_kernel_console();

    STEPControl_Writer writer;

    // OCCT reports the useful detail to its messenger, not through the
    // exception, so fold the status code and path into the message we throw.
    // It is all Task 5 has to put in Error::StepWriteFailed { reason }.
    const IFSelect_ReturnStatus transferred =
        writer.Transfer(shape.inner, STEPControl_AsIs);
    if (transferred != IFSelect_RetDone) {
      throw std::runtime_error("STEP transfer failed with status " +
                               std::to_string(static_cast<int>(transferred)));
    }

    // rust::Str is not null-terminated, so copy before handing to OCCT.
    const std::string target(path.data(), path.size());

    const IFSelect_ReturnStatus written = writer.Write(target.c_str());
    if (written != IFSelect_RetDone) {
      throw std::runtime_error("STEP write to '" + target +
                               "' failed with status " +
                               std::to_string(static_cast<int>(written)));
    }
  });
}

}  // namespace scriber
