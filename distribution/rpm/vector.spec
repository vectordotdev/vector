%define _name vector
%define _cleaned_version %{getenv:CLEANED_VERSION}
%define _release %{getenv:RELEASE}
%define _url https://vector.dev
%define _version %{getenv:VERSION}
%define _source %{_name}-%{_arch}.tar.gz
%define _sourceroot %{_name}-%{_arch}
%define _buildname %{name}-%{version}-%{release}.%{_arch}
%define _username %{_name}
%define _sharedstatedir /var/lib

%if %{undefined _unitdir}
%global _unitdir %{_prefix}/lib/systemd/system
%endif

%if %{undefined _presetdir}
%global _presetdir %{_prefix}/lib/systemd/system-preset
%endif

%if %{undefined _modulesloaddir}
%global _modulesloaddir %{_prefix}/lib/modules-load.d
%endif

%if %{undefined _systemdgeneratordir}
%global _systemdgeneratordir %{_prefix}/lib/systemd/system-generators
%endif

%define _build_id_links none

Name: %{_name}
Summary: A lightweight and ultra-fast tool for building observability pipelines
Version: %{_cleaned_version}
Release: %{_release}
License: MPL-2.0
Group: Applications/System
Source: %{_source}
URL: %{_url}

%description
%{summary}

%prep
# We are currently in the BUILD dir
tar -xvf %{_sourcedir}/%{_source} --strip-components=2
cp -a %{_sourcedir}/systemd/. systemd

%install
# We are currently in the BUILDROOT dir
rm -rf %{buildroot}
mkdir -p %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_sysconfdir}/%{_name}
mkdir -p %{buildroot}%{_sysconfdir}/default
mkdir -p %{buildroot}%{_sharedstatedir}/%{_name}
mkdir -p %{buildroot}%{_datadir}/%{_name}
mkdir -p %{buildroot}%{_unitdir}

cp -a %{_builddir}/bin/vector %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_datadir}/%{_name}/examples
cp -a %{_builddir}/config/vector.yaml %{buildroot}%{_datadir}/%{_name}/examples/vector.yaml
cp -a %{_builddir}/config/examples/. %{buildroot}%{_sysconfdir}/%{_name}/examples
cp -a %{_builddir}/systemd/vector.service %{buildroot}%{_unitdir}/vector.service
cp -a %{_builddir}/systemd/vector.default %{buildroot}%{_sysconfdir}/default/vector
cp -a %{_builddir}/licenses/. %{buildroot}%{_datadir}/%{_name}/licenses
cp -a %{_builddir}/NOTICE %{buildroot}%{_datadir}/%{_name}/NOTICE
cp -a %{_builddir}/LICENSE-3rdparty.csv %{buildroot}%{_datadir}/%{_name}/LICENSE-3rdparty.csv

# Generate shell completions; fall back to empty files if the binary cannot run
# (e.g. cross-compiled for a different architecture without QEMU/binfmt support).
mkdir -p %{buildroot}%{_datadir}/bash-completion/completions
mkdir -p %{buildroot}%{_datadir}/zsh/site-functions
mkdir -p %{buildroot}%{_datadir}/fish/vendor_completions.d
%{buildroot}%{_bindir}/vector completion bash > %{buildroot}%{_datadir}/bash-completion/completions/vector 2>/dev/null || \
  touch %{buildroot}%{_datadir}/bash-completion/completions/vector
%{buildroot}%{_bindir}/vector completion zsh  > %{buildroot}%{_datadir}/zsh/site-functions/_vector 2>/dev/null || \
  touch %{buildroot}%{_datadir}/zsh/site-functions/_vector
%{buildroot}%{_bindir}/vector completion fish > %{buildroot}%{_datadir}/fish/vendor_completions.d/vector.fish 2>/dev/null || \
  touch %{buildroot}%{_datadir}/fish/vendor_completions.d/vector.fish

%post
getent passwd %{_username} > /dev/null || \
  useradd --shell /sbin/nologin --system --home-dir %{_sharedstatedir}/%{_name} --user-group \
    --comment "Vector observability data router" %{_username}
chown %{_username} %{_sharedstatedir}/%{_name}
usermod -aG systemd-journal %{_username}  || true
usermod -aG systemd-journal-remote %{_username}  || true

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
%{_bindir}/*
%{_unitdir}/vector.service
%config(noreplace) %{_sysconfdir}/default/vector
# Older versions installed a demo config at this path; mark it as %ghost so
# rpm preserves any existing on-disk file during upgrade instead of removing
# it as orphaned.
%ghost %config(noreplace) %{_sysconfdir}/%{_name}/vector.yaml
%config %{_sysconfdir}/%{_name}/examples/*
%dir %{_sharedstatedir}/%{_name}
%doc README.md
%doc %{_datadir}/%{_name}/NOTICE
%doc %{_datadir}/%{_name}/examples/vector.yaml
%doc %{_datadir}/%{_name}/licenses/*
%doc %{_datadir}/%{_name}/LICENSE-3rdparty.csv
%license LICENSE
%{_datadir}/bash-completion/completions/vector
%{_datadir}/zsh/site-functions/_vector
%{_datadir}/fish/vendor_completions.d/vector.fish

%changelog
* Fri Jun 21 2019 Vector Devs <vector@datadoghq.com> - 0.3.0
- Release v0.3.0
