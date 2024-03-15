/// Syslog facility
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
enum Facility {
    /// Syslog facility ordinal number
    Fixed(u8),

    /// Syslog facility name
    Field(String)
}

impl Default for Facility {
    fn default() -> Self {
        Facility::Fixed(1)
    }
}

/// Syslog severity
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
enum Severity {
    /// Syslog severity ordinal number
    Fixed(u8),

    /// Syslog severity name
    Field(String)
}

impl Default for Severity {
    fn default() -> Self {
        Severity::Fixed(6)
    }
}

fn deserialize_facility<'de, D>(d: D) -> Result<Facility, D::Error>
    where D: de::Deserializer<'de>
{
    let value: String = String::deserialize(d)?;
    let num_value = value.parse::<u8>();
    match num_value {
        Ok(num) => {
            if num > 23 {
                return Err(de::Error::invalid_value(de::Unexpected::Unsigned(num as u64), &"facility number too large"));
            } else {
                return Ok(Facility::Fixed(num));
            }
        }
        Err(_) => {
            if let Some(field_name) = value.strip_prefix("$.message.") {
                return Ok(Facility::Field(field_name.to_string()));
            } else {
                let num = match value.to_uppercase().as_str() {
                    "KERN" => 0,
                    "USER" => 1,
                    "MAIL" => 2,
                    "DAEMON" => 3,
                    "AUTH" => 4,
                    "SYSLOG" => 5,
                    "LPR" => 6,
                    "NEWS" => 7,
                    "UUCP" => 8,
                    "CRON" => 9,
                    "AUTHPRIV" => 10,
                    "FTP" => 11,
                    "NTP" => 12,
                    "SECURITY" => 13,
                    "CONSOLE" => 14,
                    "SOLARIS-CRON" => 15,
                    "LOCAL0" => 16,
                    "LOCAL1" => 17,
                    "LOCAL2" => 18,
                    "LOCAL3" => 19,
                    "LOCAL4" => 20,
                    "LOCAL5" => 21,
                    "LOCAL6" => 22,
                    "LOCAL7" => 23,
                    _ => 24,
                };
                if num > 23 {
                    return Err(de::Error::invalid_value(de::Unexpected::Unsigned(num as u64), &"unknown facility"));
                } else {
                    return Ok(Facility::Fixed(num))
                }
            }
        }
    }
}

fn deserialize_severity<'de, D>(d: D) -> Result<Severity, D::Error>
    where D: de::Deserializer<'de>
{
    let value: String = String::deserialize(d)?;
    let num_value = value.parse::<u8>();
    match num_value {
        Ok(num) => {
            if num > 7 {
                return Err(de::Error::invalid_value(de::Unexpected::Unsigned(num as u64), &"severity number too large"))
            } else {
                return Ok(Severity::Fixed(num))
            }
        }
        Err(_) => {
            if let Some(field_name) = value.strip_prefix("$.message.") {
                return Ok(Severity::Field(field_name.to_string()));
            } else {
                let num = match value.to_uppercase().as_str() {
                    "EMERGENCY" => 0,
                    "ALERT" => 1,
                    "CRITICAL" => 2,
                    "ERROR" => 3,
                    "WARNING" => 4,
                    "NOTICE" => 5,
                    "INFORMATIONAL" => 6,
                    "DEBUG" => 7,
                    _ => 8,
                };
                if num > 7 {
                    return Err(de::Error::invalid_value(de::Unexpected::Unsigned(num as u64), &"unknown severity"))
                } else {
                    return Ok(Severity::Fixed(num))
                }
            }
        }
    }
}

fn get_num_facility(config_facility: &Facility, log: &LogEvent) -> u8 {
    match config_facility {
        Facility::Fixed(num) => return *num,
        Facility::Field(field_name) => {
            if let Some(field_value) = log.get(field_name.as_str()) {
                let field_value_string = String::from_utf8(field_value.coerce_to_bytes().to_vec()).unwrap_or_default();
                let num_value = field_value_string.parse::<u8>();
                match num_value {
                    Ok(num) => {
                        if num > 23 {
                            return 1 // USER
                        } else {
                            return num
                        }
                    }
                    Err(_) => {
                            let num = match field_value_string.to_uppercase().as_str() {
                                "KERN" => 0,
                                "USER" => 1,
                                "MAIL" => 2,
                                "DAEMON" => 3,
                                "AUTH" => 4,
                                "SYSLOG" => 5,
                                "LPR" => 6,
                                "NEWS" => 7,
                                "UUCP" => 8,
                                "CRON" => 9,
                                "AUTHPRIV" => 10,
                                "FTP" => 11,
                                "NTP" => 12,
                                "SECURITY" => 13,
                                "CONSOLE" => 14,
                                "SOLARIS-CRON" => 15,
                                "LOCAL0" => 16,
                                "LOCAL1" => 17,
                                "LOCAL2" => 18,
                                "LOCAL3" => 19,
                                "LOCAL4" => 20,
                                "LOCAL5" => 21,
                                "LOCAL6" => 22,
                                "LOCAL7" => 23,
                                _ => 24,
                            };
                            if num > 23 {
                                return 1 // USER
                            } else {
                                return num
                            }
                        }
                    }
            } else {
                return 1 // USER
            }
        }
    }
}

fn get_num_severity(config_severity: &Severity, log: &LogEvent) -> u8 {
    match config_severity {
        Severity::Fixed(num) => return *num,
        Severity::Field(field_name) => {
            if let Some(field_value) = log.get(field_name.as_str()) {
                let field_value_string = String::from_utf8(field_value.coerce_to_bytes().to_vec()).unwrap_or_default();
                let num_value = field_value_string.parse::<u8>();
                match num_value {
                    Ok(num) => {
                        if num > 7 {
                            return 6 // INFORMATIONAL
                        } else {
                            return num
                        }
                    }
                    Err(_) => {
                            let num = match field_value_string.to_uppercase().as_str() {
                                "EMERGENCY" => 0,
                                "ALERT" => 1,
                                "CRITICAL" => 2,
                                "ERROR" => 3,
                                "WARNING" => 4,
                                "NOTICE" => 5,
                                "INFORMATIONAL" => 6,
                                "DEBUG" => 7,
                                _ => 8,
                            };
                            if num > 7 {
                                return 6 // INFORMATIONAL
                            } else {
                                return num
                            }
                        }
                    }
            } else {
                return 6 // INFORMATIONAL
            }
        }
    }
}
