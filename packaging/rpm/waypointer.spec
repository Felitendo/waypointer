# Built with `packaging/build-rpm.sh`, which passes the version in rather than
# editing this file: Cargo.toml is where the version is written down.
%global upstream_version %{?_version}%{!?_version:0.1.1}
# A Rust binary: no debug package of its own.
%global debug_package %{nil}

Name:           waypointer
Version:        %{upstream_version}
Release:        1%{?dist}
Summary:        A tiny hand on a stick that points at things on your screen

License:        MIT
URL:            https://github.com/Felitendo/waypointer
Source0:        %{name}-%{version}.tar.gz

# cargo fetches the crates (and Rune, from git) while it builds.
BuildRequires:  cargo >= 1.88
BuildRequires:  rust >= 1.88
BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  pkgconfig(alsa)
BuildRequires:  pkgconfig(xkbcommon)

# Opened at run time, so rpm cannot see them in the binary.
Requires:       fontconfig
Requires:       hicolor-icon-theme
Requires:       libwayland-client
Requires:       vulkan-loader
Recommends:     mesa-vulkan-drivers
Recommends:     xdg-desktop-portal

%description
Turn it on and your cursor becomes a toy hand on a stick. Click to tap, drag to
draw, and nothing underneath gets clicked. Great for screen shares, recordings
and presentations. The Linux version of TinyHand for macOS.

A toolbar at the bottom picks one of 36 hands or a laser dot, the ink colour
and the volume of the tap sound. On KDE Plasma it also has global shortcuts,
holding Alt to click for real, and a double tap on Shift to let clicks through.
It needs a Wayland desktop with layer-shell, such as KDE Plasma, Hyprland or
Sway.

%prep
%autosetup -n %{name}-%{version}

%build
make build

%install
make install DESTDIR=%{buildroot} PREFIX=%{_prefix}

%files
%license %{_datadir}/licenses/%{name}/LICENSE
%doc %{_datadir}/doc/%{name}/README.md
%{_bindir}/%{name}
%{_datadir}/%{name}/
%{_datadir}/applications/de.felitendo.waypointer.desktop
%{_datadir}/icons/hicolor/*/apps/de.felitendo.waypointer.*
%{_datadir}/icons/hicolor/scalable/status/waypointer-*.svg

%changelog
