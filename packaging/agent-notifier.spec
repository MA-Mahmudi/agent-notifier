%global package_version %{?package_version}%{!?package_version:0.1.3}

Name:           agent-notifier
Version:        %{package_version}
Release:        1%{?dist}
Summary:        Local Codex and Claude Code session monitor
License:        GPL-3.0-or-later
URL:            https://github.com/MrMohebi/agent-notifier
Source0:        agent-notifier-%{version}.tar.gz
BuildArch:      x86_64

%description
Rust companion service for the Agent Notifier GNOME Shell extension.

%prep
%setup -q

%build

%install
install -Dm755 agent-notifier %{buildroot}/usr/libexec/agent-notifier
mkdir -p %{buildroot}%{_bindir}
ln -s ../libexec/agent-notifier %{buildroot}%{_bindir}/agent-notifier
install -Dm644 agent-notifier.service %{buildroot}/usr/lib/systemd/user/agent-notifier.service
install -Dm644 io.github.mmmohebi.AgentNotifier.service %{buildroot}%{_datadir}/dbus-1/services/io.github.mmmohebi.AgentNotifier.service
install -Dm644 README.md %{buildroot}%{_docdir}/agent-notifier/README.md
install -Dm644 LICENSE %{buildroot}%{_licensedir}/agent-notifier/LICENSE

%files
%{_bindir}/agent-notifier
/usr/libexec/agent-notifier
/usr/lib/systemd/user/agent-notifier.service
%{_datadir}/dbus-1/services/io.github.mmmohebi.AgentNotifier.service
%doc %{_docdir}/agent-notifier/README.md
%license %{_licensedir}/agent-notifier/LICENSE

%changelog
* Tue Sep 29 2026 Mohammad Mohebi <mmmohebi@users.noreply.github.com> - %{package_version}-1
- Initial package
