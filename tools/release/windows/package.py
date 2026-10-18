"""Windows installer payloads built from the staged portable release."""

from pathlib import Path
import os
import subprocess
import uuid
import xml.etree.ElementTree as XML

UPGRADE_CODE = "6b366343-c403-4cd1-b9a5-41cfb57d24db"
WIX_NAMESPACE = "http://wixtoolset.org/schemas/v4/wxs"


def build_windows_installers(payload, output, name, version, formats, packaging_directory):
    from release_targets import windows_package_versions

    msi_version, msix_version = windows_package_versions(version)
    if "msi" in formats:
        build_msi(payload, output / f"{name}.msi", msi_version)
    if "msix" in formats:
        build_msix(payload, output / f"{name}.msix", msix_version, packaging_directory)


def build_msi(payload, destination, version):
    XML.register_namespace("", WIX_NAMESPACE)
    def element(parent, tag, **attributes):
        return XML.SubElement(parent, f"{{{WIX_NAMESPACE}}}{tag}", attributes)
    root = XML.Element(f"{{{WIX_NAMESPACE}}}Wix")
    package = element(root, "Package", Name="OpenZT2", Manufacturer="OpenZT2 contributors",
                      Version=version, UpgradeCode=UPGRADE_CODE, Language="1033", Scope="perUser")
    element(package, "MajorUpgrade", DowngradeErrorMessage="A newer version of OpenZT2 is installed.")
    element(package, "MediaTemplate", EmbedCab="yes")
    local = element(package, "StandardDirectory", Id="LocalAppDataFolder")
    programs = element(local, "Directory", Id="LocalPrograms", Name="Programs")
    install = element(programs, "Directory", Id="INSTALLFOLDER", Name="OpenZT2")
    menu = element(package, "StandardDirectory", Id="ProgramMenuFolder")
    element(menu, "Directory", Id="OpenZT2Menu", Name="OpenZT2")
    feature = element(package, "Feature", Id="Game", Title="OpenZT2", Level="1")
    directories = {Path("."): install}
    executable_component = None
    for index, source in enumerate(sorted(payload.rglob("*"))):
        if not source.is_file():
            continue
        relative = source.relative_to(payload)
        current = Path(".")
        for part in relative.parent.parts:
            next_path = current / part
            if next_path not in directories:
                identifier = "D" + uuid.uuid5(uuid.UUID(UPGRADE_CODE), next_path.as_posix()).hex
                directories[next_path] = element(directories[current], "Directory", Id=identifier, Name=part)
            current = next_path
        identifier = f"File{index}"
        component = element(directories[current], "Component", Id=identifier,
                            Guid=str(uuid.uuid5(uuid.UUID(UPGRADE_CODE), relative.as_posix())))
        file = element(component, "File", Id=f"Payload{index}", Source=str(source), Name=source.name)
        element(component, "RegistryValue", Root="HKCU", Key="Software\\OpenZT2\\Installer",
                Name=identifier, Type="string", Value=version, KeyPath="yes")
        if relative.as_posix() == "openzt2.exe":
            executable_component = component
            element(file, "Shortcut", Id="StartMenuShortcut", Directory="OpenZT2Menu",
                    Name="OpenZT2", WorkingDirectory="INSTALLFOLDER", Advertise="no")
        element(feature, "ComponentRef", Id=identifier)
    if executable_component is None:
        raise RuntimeError("The installer payload has no openzt2.exe")
    for identifier in ["LocalPrograms", "OpenZT2Menu", *(node.get("Id") for node in directories.values())]:
        element(executable_component, "RemoveFolder", Id=f"Cleanup{identifier}",
                Directory=identifier, On="uninstall")
    source = destination.with_suffix(".wxs")
    XML.ElementTree(root).write(source, encoding="utf-8", xml_declaration=True)
    subprocess.run(["wix", "build", "-arch", "x64", "-o", str(destination), str(source)], check=True)


def build_msix(payload, destination, version, packaging_directory):
    assets = payload / "Assets"
    assets.mkdir(exist_ok=True)
    icon = packaging_directory / "linux/org.openzt2.OpenZT2.svg"
    for name, size in (("Square44x44Logo", 44), ("Square150x150Logo", 150), ("StoreLogo", 50)):
        subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size), str(icon),
                        "-o", str(assets / f"{name}.png")], check=True)
    foundation = "http://schemas.microsoft.com/appx/manifest/foundation/windows10"
    uap = "http://schemas.microsoft.com/appx/manifest/uap/windows10"
    rescap = "http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
    desktop6 = "http://schemas.microsoft.com/appx/manifest/desktop/windows10/6"
    XML.register_namespace("", foundation)
    XML.register_namespace("uap", uap)
    XML.register_namespace("rescap", rescap)
    XML.register_namespace("desktop6", desktop6)
    root = XML.Element(f"{{{foundation}}}Package", {"IgnorableNamespaces": "uap rescap desktop6"})
    def element(parent, tag, **attributes):
        return XML.SubElement(parent, f"{{{foundation}}}{tag}", attributes)
    element(root, "Identity", Name="OpenZT2.OpenZT2", ProcessorArchitecture="x64", Version=version,
            Publisher=os.environ.get("OPENZT2_MSIX_PUBLISHER") or "CN=OpenZT2")
    properties = element(root, "Properties")
    for tag, text in (("DisplayName", "OpenZT2"), ("PublisherDisplayName", "OpenZT2 contributors"),
                      ("Logo", "Assets\\StoreLogo.png")):
        element(properties, tag).text = text
    XML.SubElement(properties, f"{{{desktop6}}}FileSystemWriteVirtualization").text = "disabled"
    element(element(root, "Resources"), "Resource", Language="en-us")
    element(element(root, "Dependencies"), "TargetDeviceFamily", Name="Windows.Desktop",
            MinVersion="10.0.19041.0", MaxVersionTested="10.0.26100.0")
    application = element(element(root, "Applications"), "Application", Id="OpenZT2",
                          Executable="openzt2.exe", EntryPoint="Windows.FullTrustApplication")
    XML.SubElement(application, f"{{{uap}}}VisualElements", {
        "DisplayName": "OpenZT2", "Description": "Zoo simulation using your Zoo Tycoon 2 game files",
        "BackgroundColor": "transparent", "Square44x44Logo": "Assets\\Square44x44Logo.png",
        "Square150x150Logo": "Assets\\Square150x150Logo.png"})
    capabilities = element(root, "Capabilities")
    XML.SubElement(capabilities, f"{{{rescap}}}Capability", {"Name": "runFullTrust"})
    XML.SubElement(capabilities, f"{{{rescap}}}Capability", {"Name": "unvirtualizedResources"})
    XML.ElementTree(root).write(payload / "AppxManifest.xml", encoding="utf-8", xml_declaration=True)
    subprocess.run(["makeappx.exe", "pack", "/d", str(payload), "/p", str(destination), "/o"], check=True)
    subprocess.run(["pwsh", "-NoProfile", "-File", str(packaging_directory / "windows/sign-msix.ps1"),
                    "-PackagePath", str(destination)], check=True)
