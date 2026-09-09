use std::{error::Error, fmt};

#[cfg(unix)]
use std::{env, fs, path::Path};

use vrl::compiler::TimeZone;

/// An error returned when the system local time zone cannot be loaded.
#[derive(Debug, Eq, PartialEq)]
pub struct LocalTimeZoneError {
    source: String,
}

impl fmt::Display for LocalTimeZoneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Unable to load the system local time zone: {}. Set `timezone` explicitly to a valid IANA time zone or configure system time zone data.",
            self.source
        )
    }
}

impl Error for LocalTimeZoneError {}

/// Verifies that a configured time zone can be loaded.
///
/// Named time zones are embedded in the binary and require no system lookup.
/// The system local time zone is loaded fallibly so that callers do not inherit
/// Chrono's silent fallback to UTC.
///
/// # Errors
///
/// Returns an error when `timezone` is local and the system local time zone
/// cannot be loaded.
pub fn validate_timezone(timezone: TimeZone) -> Result<(), LocalTimeZoneError> {
    validate_timezone_with(timezone, validate_system_timezone)
}

#[cfg(unix)]
fn validate_system_timezone() -> Result<(), String> {
    match env::var("TZ") {
        Ok(timezone) => validate_chrono_timezone(&timezone).or_else(|timezone_error| {
            validate_chrono_fallback()
                .map_err(|fallback_error| format!("{timezone_error}; {fallback_error}"))
        }),
        Err(env::VarError::NotPresent | env::VarError::NotUnicode(_)) => {
            validate_chrono_timezone("localtime").or_else(|localtime_error| {
                validate_chrono_fallback()
                    .map_err(|fallback_error| format!("{localtime_error}; {fallback_error}"))
            })
        }
    }
}

#[cfg(windows)]
fn validate_system_timezone() -> Result<(), String> {
    std::panic::catch_unwind(chrono::Local::now)
        .map(|_| ())
        .map_err(|_| "Chrono could not load the Windows system time zone".into())
}

#[cfg(not(any(unix, windows)))]
fn validate_system_timezone() -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn validate_chrono_fallback() -> Result<(), String> {
    let timezone = iana_time_zone::get_timezone()
        .map_err(|error| format!("fallback time zone discovery failed: {error}"))?;
    load_chrono_timezone(&timezone, CHRONO_FALLBACK_ZONEINFO_DIRECTORIES)
}

#[cfg(unix)]
fn validate_chrono_timezone(timezone: &str) -> Result<(), String> {
    if timezone.is_empty() {
        return Ok(());
    }
    if timezone == "localtime" {
        return load_chrono_timezone_file(timezone, Path::new("/etc/localtime"));
    }

    if let Some(timezone) = timezone.strip_prefix(':') {
        return load_chrono_timezone_file_search(timezone, CHRONO_ZONEINFO_DIRECTORIES);
    }

    if load_chrono_timezone(timezone, CHRONO_ZONEINFO_DIRECTORIES).is_ok() {
        return Ok(());
    }

    validate_chrono_posix_timezone(timezone.trim_matches(|c: char| c.is_ascii_whitespace()))
}

#[cfg(unix)]
fn validate_chrono_posix_timezone(timezone: &str) -> Result<(), String> {
    jiff::tz::TimeZone::posix(timezone)
        .map_err(|error| format!("invalid POSIX time zone `{timezone}`: {error}"))?;

    // Chrono deliberately disables the TZ string extensions that permit signed
    // transition times and hours outside 0..=24, and limits abbreviations to
    // seven bytes. Jiff accepts those extensions, so enforce Chrono's narrower
    // rules after using Jiff for the grammar.
    validate_chrono_posix_time_zone_types(timezone)?;

    let mut rules = timezone.rsplitn(3, ',');
    let end = rules.next();
    let start = rules.next();
    if rules.next().is_some() {
        for rule in [start, end].into_iter().flatten() {
            if let Some((_, time)) = rule.rsplit_once('/') {
                validate_chrono_rule_time(time)?;
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
fn validate_chrono_abbreviation(timezone: &str) -> Result<&str, String> {
    let (abbreviation, remainder) = if let Some(quoted) = timezone.strip_prefix('<') {
        let (abbreviation, remainder) = quoted
            .split_once('>')
            .expect("Jiff already validated the quoted abbreviation");
        (abbreviation, remainder)
    } else {
        let end = timezone
            .find(|character: char| !character.is_ascii_alphabetic())
            .unwrap_or(timezone.len());
        timezone.split_at(end)
    };

    if abbreviation.len() > 7 {
        return Err(format!(
            "POSIX time zone abbreviation `{abbreviation}` exceeds Chrono's seven-byte limit"
        ));
    }

    Ok(remainder)
}

#[cfg(unix)]
fn parse_chrono_posix_offset(timezone: &str) -> Result<(i32, &str), String> {
    let (sign, unsigned) = if let Some(unsigned) = timezone.strip_prefix('+') {
        (1, unsigned)
    } else if let Some(unsigned) = timezone.strip_prefix('-') {
        (-1, unsigned)
    } else {
        (1, timezone)
    };
    let end = unsigned
        .find(|character: char| !character.is_ascii_digit() && character != ':')
        .unwrap_or(unsigned.len());
    let (offset, remainder) = unsigned.split_at(end);
    let mut parts = offset.split(':');
    let hour = parts
        .next()
        .and_then(|part| part.parse::<i32>().ok())
        .expect("Jiff already validated the POSIX offset");
    let minute = parts
        .next()
        .map_or(0, |part| part.parse::<i32>().expect("validated minute"));
    let second = parts
        .next()
        .map_or(0, |part| part.parse::<i32>().expect("validated second"));
    let offset = sign * (hour * 3600 + minute * 60 + second);

    if chrono::FixedOffset::east_opt(-offset).is_none() {
        return Err(format!(
            "POSIX UTC offset `{}` is unsupported by Chrono",
            timezone
                .strip_suffix(remainder)
                .expect("remainder belongs to the POSIX offset")
        ));
    }

    Ok((offset, remainder))
}

#[cfg(unix)]
fn validate_chrono_rule_time(time: &str) -> Result<(), String> {
    let mut parts = time.split(':');
    let hour = parts.next().and_then(|part| part.parse::<u16>().ok());
    let minute = parts.next().map_or(Some(0), |part| part.parse::<u8>().ok());
    let second = parts.next().map_or(Some(0), |part| part.parse::<u8>().ok());

    if parts.next().is_some()
        || !matches!(hour, Some(0..=24))
        || !matches!(minute, Some(0..=59))
        || !matches!(second, Some(0..=59))
    {
        return Err(format!(
            "POSIX transition time `{time}` uses extensions unsupported by Chrono"
        ));
    }

    Ok(())
}

#[cfg(unix)]
const CHRONO_ZONEINFO_DIRECTORIES: &[&str] = &[
    "/usr/share/zoneinfo",
    "/share/zoneinfo",
    "/etc/zoneinfo",
    "/usr/share/lib/zoneinfo",
];

#[cfg(all(unix, target_os = "aix"))]
const CHRONO_FALLBACK_ZONEINFO_DIRECTORIES: &[&str] = &["/usr/share/lib/zoneinfo"];

#[cfg(all(
    unix,
    not(any(target_os = "aix", target_os = "android", target_env = "ohos"))
))]
const CHRONO_FALLBACK_ZONEINFO_DIRECTORIES: &[&str] = &["/usr/share/zoneinfo"];

#[cfg(all(unix, any(target_os = "android", target_env = "ohos")))]
const CHRONO_FALLBACK_ZONEINFO_DIRECTORIES: &[&str] = &[];

#[cfg(unix)]
fn load_chrono_timezone(timezone: &str, directories: &[&str]) -> Result<(), String> {
    #[cfg(any(target_os = "android", target_env = "ohos"))]
    if load_chrono_mobile_timezone(timezone).is_ok() {
        return Ok(());
    }

    load_chrono_timezone_file_search(timezone, directories)
}

#[cfg(unix)]
fn load_chrono_timezone_file_search(timezone: &str, directories: &[&str]) -> Result<(), String> {
    let timezone_path = Path::new(timezone);
    let paths = directories
        .iter()
        .map(|directory| Path::new(directory).join(timezone))
        .chain(
            timezone_path
                .is_absolute()
                .then(|| timezone_path.to_owned()),
        );

    for path in paths {
        if load_chrono_timezone_file(timezone, &path).is_ok() {
            return Ok(());
        }
    }

    Err(format!(
        "fallback time zone `{timezone}` could not be loaded from Chrono's zoneinfo directories"
    ))
}

#[cfg(target_os = "android")]
fn load_chrono_mobile_timezone(timezone: &str) -> Result<(), String> {
    for (environment, suffix) in [
        ("ANDROID_DATA", "/misc/zoneinfo/tzdata"),
        ("ANDROID_ROOT", "/usr/share/zoneinfo/tzdata"),
    ] {
        if let Ok(prefix) = env::var(environment) {
            let path = format!("{prefix}{suffix}");
            if let Ok(()) = load_chrono_concatenated_timezone(timezone, Path::new(&path), 52) {
                return Ok(());
            }
        }
    }

    Err(format!(
        "Chrono could not load `{timezone}` from the Android time zone database"
    ))
}

#[cfg(target_env = "ohos")]
fn load_chrono_mobile_timezone(timezone: &str) -> Result<(), String> {
    load_chrono_concatenated_timezone(timezone, Path::new("/system/etc/zoneinfo/tzdata"), 48)
}

#[cfg(any(target_os = "android", target_env = "ohos"))]
fn load_chrono_concatenated_timezone(
    timezone: &str,
    path: &Path,
    entry_length: usize,
) -> Result<(), String> {
    const HEADER_LENGTH: usize = 24;
    const NAME_LENGTH: usize = 40;

    let data = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if data.len() < HEADER_LENGTH || !data.starts_with(b"tzdata") || data[11] != 0 {
        return Err(format!("{}: invalid tzdata header", path.display()));
    }

    let read_offset = |range: std::ops::Range<usize>| {
        data.get(range)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_be_bytes)
            .map(|offset| offset as usize)
    };
    let index_offset = read_offset(12..16)
        .ok_or_else(|| format!("{}: invalid tzdata index offset", path.display()))?;
    let data_offset = read_offset(16..20)
        .ok_or_else(|| format!("{}: invalid tzdata data offset", path.display()))?;
    let index = data
        .get(index_offset..data_offset)
        .ok_or_else(|| format!("{}: invalid tzdata index", path.display()))?;

    for entry in index.chunks_exact(entry_length) {
        let name_end = entry[..NAME_LENGTH]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| format!("{}: invalid tzdata time zone name", path.display()))?;
        if &entry[..name_end] != timezone.as_bytes() {
            continue;
        }

        let offset =
            u32::from_be_bytes(entry[NAME_LENGTH..NAME_LENGTH + 4].try_into().unwrap()) as usize;
        let length = u32::from_be_bytes(entry[NAME_LENGTH + 4..NAME_LENGTH + 8].try_into().unwrap())
            as usize;
        let start = data_offset
            .checked_add(offset)
            .ok_or_else(|| format!("{}: invalid tzdata entry offset", path.display()))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| format!("{}: invalid tzdata entry length", path.display()))?;
        let tzif = data
            .get(start..end)
            .ok_or_else(|| format!("{}: invalid tzdata entry", path.display()))?;
        return validate_chrono_tzif(timezone, tzif)
            .map_err(|error| format!("{}: {error}", path.display()));
    }

    Err(format!(
        "{}: time zone `{timezone}` was not found",
        path.display()
    ))
}

#[cfg(unix)]
fn load_chrono_timezone_file(timezone: &str, path: &Path) -> Result<(), String> {
    let data = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    validate_chrono_tzif(timezone, &data).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(unix)]
fn validate_chrono_tzif(timezone: &str, data: &[u8]) -> Result<(), String> {
    jiff::tz::TimeZone::tzif(timezone, data).map_err(|error| error.to_string())?;

    let first_version = *data
        .get(4)
        .ok_or_else(|| "truncated TZif header".to_string())?;
    let (version, first_end) = validate_chrono_tzif_block(data, 4, first_version == 0)?;
    if version == 0 {
        return Ok(());
    }

    let (second_version, second_end) = validate_chrono_tzif_block(
        data.get(first_end..)
            .ok_or_else(|| "missing second TZif data block".to_string())?,
        8,
        true,
    )?;
    let footer = data
        .get(first_end + second_end..)
        .ok_or_else(|| "missing TZif footer".to_string())?;
    let footer = std::str::from_utf8(footer)
        .map_err(|error| format!("invalid UTF-8 in TZif footer: {error}"))?;
    let rule = footer.trim_matches(|character: char| character.is_ascii_whitespace());
    if !rule.is_empty() {
        if second_version == b'2' {
            validate_chrono_posix_timezone(rule)?;
        } else {
            validate_chrono_posix_time_zone_types(rule)?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn validate_chrono_tzif_block(
    data: &[u8],
    time_size: usize,
    check_chrono_constraints: bool,
) -> Result<(u8, usize), String> {
    const HEADER_SIZE: usize = 44;

    let header = data
        .get(..HEADER_SIZE)
        .ok_or_else(|| "truncated TZif header".to_string())?;
    if &header[..4] != b"TZif" {
        return Err("invalid TZif magic number".into());
    }
    let version = header[4];
    if !matches!(version, 0 | b'2' | b'3') {
        return Err(format!(
            "TZif version `{}` is unsupported by Chrono",
            char::from(version)
        ));
    }
    let count =
        |offset: usize| u32::from_be_bytes(header[offset..offset + 4].try_into().unwrap()) as usize;
    let ut_local_count = count(20);
    let std_wall_count = count(24);
    let leap_count = count(28);
    let transition_count = count(32);
    let type_count = count(36);
    let char_count = count(40);

    let transition_times_size = transition_count
        .checked_mul(time_size)
        .ok_or_else(|| "invalid TZif transition count".to_string())?;
    let transition_times_start = HEADER_SIZE;
    let transition_types_start = transition_times_start
        .checked_add(transition_times_size)
        .ok_or_else(|| "invalid TZif data size".to_string())?;
    let local_time_types_start = HEADER_SIZE
        .checked_add(transition_times_size)
        .and_then(|offset| offset.checked_add(transition_count))
        .ok_or_else(|| "invalid TZif data size".to_string())?;
    let local_time_types_size = type_count
        .checked_mul(6)
        .ok_or_else(|| "invalid TZif local time type count".to_string())?;
    let names_start = local_time_types_start
        .checked_add(local_time_types_size)
        .ok_or_else(|| "invalid TZif data size".to_string())?;
    let names_end = names_start
        .checked_add(char_count)
        .ok_or_else(|| "invalid TZif name data size".to_string())?;
    let names = data
        .get(names_start..names_end)
        .ok_or_else(|| "truncated TZif name data".to_string())?;
    let local_time_types = data
        .get(local_time_types_start..names_start)
        .ok_or_else(|| "truncated TZif local time type data".to_string())?;

    if check_chrono_constraints {
        let transition_times = data
            .get(transition_times_start..transition_types_start)
            .ok_or_else(|| "truncated TZif transition data".to_string())?;
        let transition_types = data
            .get(transition_types_start..local_time_types_start)
            .ok_or_else(|| "truncated TZif transition type data".to_string())?;
        validate_chrono_tzif_transitions(
            transition_times,
            transition_types,
            time_size,
            type_count,
        )?;
        validate_chrono_tzif_types(local_time_types, names)?;
    }

    let leap_size = leap_count
        .checked_mul(time_size + 4)
        .ok_or_else(|| "invalid TZif leap second count".to_string())?;
    let leap_end = names_end
        .checked_add(leap_size)
        .ok_or_else(|| "invalid TZif data size".to_string())?;
    let std_walls_end = leap_end
        .checked_add(std_wall_count)
        .ok_or_else(|| "invalid TZif data size".to_string())?;
    let end = std_walls_end
        .checked_add(ut_local_count)
        .ok_or_else(|| "invalid TZif data size".to_string())?;
    if end > data.len() {
        return Err("truncated TZif data block".into());
    }

    if check_chrono_constraints {
        validate_chrono_tzif_leaps(&data[names_end..leap_end], time_size)?;
        let std_walls = &data[leap_end..std_walls_end];
        let ut_locals = &data[std_walls_end..end];
        validate_chrono_tzif_indicators(std_walls, ut_locals, type_count)?;
    }

    Ok((version, end))
}

#[cfg(unix)]
fn validate_chrono_tzif_transitions(
    transition_times: &[u8],
    transition_types: &[u8],
    time_size: usize,
    type_count: usize,
) -> Result<(), String> {
    let mut previous_transition = None;
    for (transition, local_time_type) in transition_times
        .chunks_exact(time_size)
        .zip(transition_types)
    {
        let transition = if time_size == 4 {
            i64::from(i32::from_be_bytes(transition.try_into().unwrap()))
        } else {
            i64::from_be_bytes(transition.try_into().unwrap())
        };
        if usize::from(*local_time_type) >= type_count
            || previous_transition.is_some_and(|previous| previous >= transition)
        {
            return Err("invalid TZif transition".into());
        }
        previous_transition = Some(transition);
    }
    Ok(())
}

#[cfg(unix)]
fn validate_chrono_tzif_types(local_time_types: &[u8], names: &[u8]) -> Result<(), String> {
    for local_time_type in local_time_types.chunks_exact(6) {
        let offset = i32::from_be_bytes(local_time_type[..4].try_into().unwrap());
        if chrono::FixedOffset::east_opt(offset).is_none() {
            return Err(format!(
                "TZif UTC offset `{offset}` is unsupported by Chrono"
            ));
        }
        if !matches!(local_time_type[4], 0 | 1) {
            return Err("invalid TZif DST indicator".into());
        }

        let designation_index = local_time_type[5] as usize;
        let name = names
            .get(designation_index..)
            .and_then(|name| {
                name.iter()
                    .position(|byte| *byte == 0)
                    .map(|end| &name[..end])
            })
            .ok_or_else(|| "invalid TZif time zone name index".to_string())?;
        if !name.is_empty()
            && (!(3..=7).contains(&name.len())
                || !name
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-')))
        {
            return Err(format!(
                "TZif time zone designation `{}` is unsupported by Chrono",
                String::from_utf8_lossy(name)
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn validate_chrono_tzif_leaps(leap_seconds: &[u8], time_size: usize) -> Result<(), String> {
    let mut previous_leap: Option<(i64, i32)> = None;
    for leap in leap_seconds.chunks_exact(time_size + 4) {
        let timestamp = if time_size == 4 {
            i64::from(i32::from_be_bytes(leap[..time_size].try_into().unwrap()))
        } else {
            i64::from_be_bytes(leap[..time_size].try_into().unwrap())
        };
        let correction = i32::from_be_bytes(leap[time_size..].try_into().unwrap());
        let valid = previous_leap.map_or(
            timestamp >= 0 && correction.saturating_abs() == 1,
            |(previous_timestamp, previous_correction)| {
                timestamp.saturating_sub(previous_timestamp) >= 2_419_199
                    && correction
                        .saturating_sub(previous_correction)
                        .saturating_abs()
                        == 1
            },
        );
        if !valid {
            return Err("invalid TZif leap second".into());
        }
        previous_leap = Some((timestamp, correction));
    }
    Ok(())
}

#[cfg(unix)]
fn validate_chrono_tzif_indicators(
    std_walls: &[u8],
    ut_locals: &[u8],
    type_count: usize,
) -> Result<(), String> {
    for index in 0..type_count {
        let standard = std_walls.get(index).copied().unwrap_or(0);
        let universal = ut_locals.get(index).copied().unwrap_or(0);
        if (standard, universal) == (0, 1) {
            return Err("invalid TZif standard/wall and UT/local indicators".into());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn validate_chrono_posix_time_zone_types(timezone: &str) -> Result<(), String> {
    let remainder = validate_chrono_abbreviation(timezone)?;
    let (standard_offset, remainder) = parse_chrono_posix_offset(remainder)?;
    if remainder.starts_with(|character: char| character.is_ascii_alphabetic() || character == '<')
    {
        let remainder = validate_chrono_abbreviation(remainder)?;
        if remainder.starts_with(',') {
            let daylight_offset = standard_offset - 3600;
            if chrono::FixedOffset::east_opt(-daylight_offset).is_none() {
                return Err("default POSIX daylight offset is unsupported by Chrono".into());
            }
        } else {
            parse_chrono_posix_offset(remainder)?;
        }
    }
    Ok(())
}

fn validate_timezone_with(
    timezone: TimeZone,
    validate_local: impl FnOnce() -> Result<(), String>,
) -> Result<(), LocalTimeZoneError> {
    if !matches!(timezone, TimeZone::Local) {
        return Ok(());
    }

    validate_local().map_err(|source| LocalTimeZoneError { source })
}

#[cfg(test)]
mod tests {
    use chrono_tz::Tz;

    use super::*;

    #[cfg(unix)]
    fn tzif_block(version: u8, designation: &[u8]) -> Vec<u8> {
        let mut tzif = Vec::from(&b"TZif\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"[..]);
        tzif[4] = version;
        for count in [0_u32, 0, 0, 0, 1, u32::try_from(designation.len()).unwrap()] {
            tzif.extend_from_slice(&count.to_be_bytes());
        }
        tzif.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        tzif.extend_from_slice(designation);
        tzif
    }

    #[test]
    fn named_timezone_does_not_load_system_timezone() {
        validate_timezone_with(TimeZone::Named(Tz::UTC), || {
            panic!("named time zones do not need system timezone data")
        })
        .unwrap();
    }

    #[test]
    fn unavailable_local_timezone_is_rejected() {
        let error = validate_timezone_with(TimeZone::Local, || {
            Err("timezone data is unavailable".into())
        })
        .unwrap_err();

        assert_eq!(
            error,
            LocalTimeZoneError {
                source: "timezone data is unavailable".into()
            }
        );
        assert_eq!(
            error.to_string(),
            "Unable to load the system local time zone: timezone data is unavailable. Set `timezone` explicitly to a valid IANA time zone or configure system time zone data."
        );
    }

    #[test]
    #[cfg(unix)]
    fn chrono_compatible_posix_timezone_is_accepted() {
        validate_chrono_posix_timezone("EST5EDT,M3.2.0,M11.1.0").unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn chrono_incompatible_posix_timezone_is_rejected() {
        let error = validate_chrono_posix_timezone("EST5EDT,0/0,J365/25").unwrap_err();
        assert_eq!(
            error,
            "POSIX transition time `25` uses extensions unsupported by Chrono"
        );
    }

    #[test]
    #[cfg(unix)]
    fn chrono_incompatible_posix_abbreviation_is_rejected() {
        let error = validate_chrono_posix_timezone("<ABCDEFGH>0").unwrap_err();
        assert_eq!(
            error,
            "POSIX time zone abbreviation `ABCDEFGH` exceeds Chrono's seven-byte limit"
        );

        let error = validate_chrono_posix_timezone("EST5<ABCDEFGH>,M3.2.0,M11.1.0").unwrap_err();
        assert_eq!(
            error,
            "POSIX time zone abbreviation `ABCDEFGH` exceeds Chrono's seven-byte limit"
        );
    }

    #[test]
    #[cfg(unix)]
    fn chrono_incompatible_posix_offset_is_rejected() {
        let error = validate_chrono_posix_timezone("EST24").unwrap_err();
        assert_eq!(error, "POSIX UTC offset `24` is unsupported by Chrono");

        let error = validate_chrono_posix_timezone("EST-23:59:59EDT,M3.2.0,M11.1.0").unwrap_err();
        assert_eq!(
            error,
            "default POSIX daylight offset is unsupported by Chrono"
        );
    }

    #[test]
    #[cfg(unix)]
    fn chrono_incompatible_tzif_designation_is_rejected() {
        let tzif = tzif_block(0, b"ABCDEFGH\0");

        let error = validate_chrono_tzif("Test/Long", &tzif).unwrap_err();
        assert_eq!(
            error,
            "TZif time zone designation `ABCDEFGH` is unsupported by Chrono"
        );
    }

    #[test]
    #[cfg(unix)]
    fn chrono_incompatible_tzif_dst_indicator_is_rejected() {
        let mut tzif = tzif_block(0, b"UTC\0");
        tzif[48] = 2;

        let error = validate_chrono_tzif("Test/Dst", &tzif).unwrap_err();
        assert_eq!(error, "invalid TZif DST indicator");
    }

    #[test]
    #[cfg(unix)]
    fn chrono_incompatible_tzif_version_is_rejected() {
        let mut tzif = tzif_block(b'4', b"UTC\0");
        tzif.extend_from_slice(&tzif_block(b'4', b"UTC\0"));
        tzif.extend_from_slice(b"\n\n");

        let error = validate_chrono_tzif("Test/V4", &tzif).unwrap_err();
        assert_eq!(error, "TZif version `4` is unsupported by Chrono");
    }

    #[test]
    #[cfg(unix)]
    fn chrono_incompatible_v2_footer_extension_is_rejected() {
        let mut tzif = tzif_block(b'2', b"EST\0EDT\0");
        tzif.extend_from_slice(&tzif_block(b'2', b"EST\0EDT\0"));
        tzif.extend_from_slice(b"\nEST5EDT,J1/-1,J365/25\n");

        let error = validate_chrono_tzif("Test/V2", &tzif).unwrap_err();
        assert_eq!(
            error,
            "POSIX transition time `-1` uses extensions unsupported by Chrono"
        );
    }
}
