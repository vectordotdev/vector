%define _name vector
%define _cleaned_version %{getenv:CLEANED_VERSION}
%define _release %{getenv:RELEASE}
%define _url https://vectorproject.io
%define _version %{getenv:VERSION}
%define _source %{_name}-%{_version}.tar.gz
%define _sourceroot %{_name}-%{_version}
%define _buildname %{name}-%{version}-%{release}.%{_arch}

Name: %{_name}
Summary: A High-Performance Logs, Metrics, and Events Routing Layer
Version: %{_cleaned_version}
Release: %{_release}
License: ASL 2.0
Group: Applications/System
Source: %{_source}
URL: %{_url}

%description
%{summary}

%prep
# We are currently in the BUILD dir
rm -rf %{_buildname}
tar -xvf %{_sourcedir}/%{_source} --strip-components=1
chown -R root.root .
chmod -R a+rX,g-w,o-w .

%install
# We are currently in the BUILDROOT dir
rm -rf %{buildroot}
mkdir -p %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_sysconfdir}/%{_name}
mkdir -p %{buildroot}%{_datadir}/%{_name}
cp -a %{_builddir}/bin/. %{buildroot}%{_bindir}
cp -a %{_builddir}/config/vector.toml %{buildroot}%{_sysconfdir}/%{_name}/vector.toml
cp -a %{_builddir}/config/vector.spec.toml %{buildroot}%{_sysconfdir}/%{_name}/vector.spec.toml
cp -a %{_builddir}/config/examples/. %{buildroot}%{_sysconfdir}/%{_name}/examples

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
%{_bindir}/*
%config(noreplace) %{_sysconfdir}/%{_name}/vector.toml
%config %{_sysconfdir}/%{_name}/vector.spec.toml
%config %{_sysconfdir}/%{_name}/examples/*
%doc README.md
%license LICENSE

%changelog
* Fri Jun 21 2019 Vector Devs <vector@timber.io> - 0.3.0
- Release v0.3.0
