%define name vector
%define release 1
%define url https://vectorproject.io
%define version %{getenv:VERSION}
%define source %{name}-%{version}.tar.gz
%define _buildroot %{name}-%{version}

Name: %{name}
Summary: A High-Performance Logs, Metrics, and Events Routing Layer
Version: %{version}
Release: %{release}
License: ASL 2.0
Group: Applications/System
Source: %{source}
URL: %{url}
BuildRoot: %{_buildroot}

%description
%{summary}

%prep]
%setup -q -n %{name}-%{version}

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a * %{buildroot}

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
