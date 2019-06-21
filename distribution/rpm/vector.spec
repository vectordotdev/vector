%define _name vector
%define _cleaned_version %{getenv:CLEANED_VERSION}
%define _release 1
%define _url https://vectorproject.io
%define _version %{getenv:VERSION}
%define _source %{_name}-%{_version}.tar.gz
%define _buildroot %{_name}-%{_version}

Name: %{_name}
Summary: A High-Performance Logs, Metrics, and Events Routing Layer
Version: %{_cleaned_version}
Release: %{_release}
License: ASL 2.0
Group: Applications/System
Source: %{_source}
URL: %{_url}
BuildRoot: %{_buildroot}

%description
%{summary}

%prep
# We are currently in the BUILD dir
rm -rf %{_buildroot}
tar -xvf %{_sourcedir}/%{_source}
cd %{_buildroot}
chown -R root.root .
chmod -R a+rX,g-w,o-w .

%install
# We are currently in the BUILDROOT dir
rm -rf %{buildroot}
mkdir -p %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_sysconfdir}/%{_name}
mkdir -p %{buildroot}%{_datadir}/%{_name}
echo $(pwd)
cp -a %{_builddir}/%{_buildroot}/bin/. %{buildroot}%{_bindir}
rm -rf %{_builddir}/%{_buildroot}/bin
cp -a %{_builddir}/%{_buildroot}/config/. %{buildroot}%{_sysconfdir}/%{_name}
rm -rf %{_builddir}/%{_buildroot}/config
cp -a %{_builddir}/%{_buildroot}/. %{buildroot}

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
%{_bindir}/*
%config(noreplace) %{_sysconfdir}/%{_name}/vector.toml
%config %{_sysconfdir}/%{_name}/vector.spec.toml
%config %{_sysconfdir}/%{_name}/examples/*
%doc README.md
%doc %{_sysconfdir}/%{_name}/vector.spec.toml
%license LICENSE

%changelog
* Fri Jun 21 2019 Vector Devs <vector@timber.io> - 0.3.0
- Release v0.3.0
