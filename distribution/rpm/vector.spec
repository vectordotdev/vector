%define _name vector
%define _cleaned_version %{getenv:CLEANED_VERSION}
%define _release 1
%define _url https://vectorproject.io
%define _version %{getenv:VERSION}
%define _source %{name}-%{_version}.tar.gz
%define _buildroot %{name}-%{)version}

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
tar -xvf %{_sourcedir}/%{_source}
cd %{_name}-%{_version}
chown -R root.root .
chmod -R a+rX,g-w,o-w .

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_sysconfdir}/%{_name}
mkdir -p %{buildroot}%{_datadir}/%{_name}
cp -a bin/* %{buildroot}%{_bindir}
cp -a config/* %{buildroot}%{_sysconfdir}/%{_name}

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
%{_bindir}/*
%doc README.md
%doc /etc/vector.spec.toml
%license LICENSE
%config(noreplace) /etc/vector.toml
%config /etc/vector.spec.toml
%config /etc/examples./*

%changelog
* Fri Jun 21 2019 Vector Devs <vector@timber.io> - 0.3.0
- Release v0.3.0
