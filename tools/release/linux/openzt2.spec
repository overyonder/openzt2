Name: openzt2
Version: %{openzt2_version}
Release: 1
Summary: Zoo simulation using user-supplied Zoo Tycoon 2 archives
License: MIT AND BSD-3-Clause AND LGPL-2.1-or-later AND Zlib
URL: https://github.com/overyonder/openzt2
BuildArch: %{openzt2_arch}
# Keep the private shader translators private; ELF dependency scanning still
# supplies the versioned system-library requirements for every shipped ELF.
%global __provides_exclude_from ^/usr/lib/openzt2/.*$
%global __requires_exclude ^(libvkd3d-shader|libmojoshader)\.so.*$
Requires: libvulkan.so.1()(64bit)
Requires: libfontconfig.so.1()(64bit)
Requires: libwayland-client.so.0()(64bit)
Requires: libxkbcommon.so.0()(64bit)
# Cargo already stripped the shipping executable. Do not rebuild debug data or
# transform the prebuilt payload during packaging.
%global debug_package %{nil}
%global __os_install_post %{nil}

%description
Early development build. Requires user-supplied Zoo Tycoon 2 archives.
See the bundled notices for the licenses of third-party dependencies.

%install
mkdir -p %{buildroot}
cp -a %{openzt2_payload}/usr %{buildroot}/

%files
%defattr(-,root,root,-)
/usr/bin/openzt2
/usr/bin/openzt2-launch
/usr/lib/openzt2
/usr/share/applications/org.openzt2.OpenZT2.desktop
/usr/share/icons/hicolor/scalable/apps/org.openzt2.OpenZT2.svg
/usr/share/metainfo/org.openzt2.OpenZT2.metainfo.xml
%doc /usr/share/doc/openzt2
