# ODBC Development

## Setup

#### MacOS

```shell
brew install unixodbc

brew install mariadb-connector-odbc@3.2.6  # Install MariaDB ODBC, and you need to configure odbcinst.ini.
```

Refs

- MySQL Connector/ODBC: <https://dev.mysql.com/doc/connector-odbc/en/connector-odbc-installation-binary-macos.html>
- MariaDB Connector/ODBC: <https://mariadb.com/kb/en/about-mariadb-connector-odbc/>
  - Homebrew mariadb-connector-odbc: <https://formulae.brew.sh/formula/mariadb-connector-odbc>
  - ODBC Configuration: <https://mariadb.com/kb/en/creating-a-data-source-with-mariadb-connectorodbc/>
    ```shell
    cat << EOF >> /opt/homebrew/etc/odbcinst.ini

    [MariaDB ODBC 3.0 Driver]
    Description = MariaDB Connector/ODBC v.3.0
    Driver = /opt/homebrew/Cellar/mariadb-connector-odbc/3.2.6/lib/mariadb/libmaodbc.dylib
    EOF
    ```
- MSSQL
  Connector/ODBC: <https://learn.microsoft.com/ko-kr/sql/connect/odbc/linux-mac/install-microsoft-odbc-driver-sql-server-macos?view=sql-server-ver16>

## ODBC Tips

Show ODBC configuration

```shell
odbcinst -j

### Output Example ###
# unixODBC 2.3.12
# DRIVERS............: /opt/homebrew/etc/odbcinst.ini
# SYSTEM DATA SOURCES: /opt/homebrew/etc/odbc.ini
# FILE DATA SOURCES..: /opt/homebrew/etc/ODBCDataSources
# USER DATA SOURCES..: /Users/<username>/.odbc.ini
# SQLULEN Size.......: 8
# SQLLEN Size........: 8
# SQLSETPOSIROW Size.: 8
```
