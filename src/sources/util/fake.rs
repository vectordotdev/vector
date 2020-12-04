use chrono::{prelude::Local, SecondsFormat};
use fakedata_generator::{gen_domain, gen_http_method, gen_ipv4, gen_username};
use lazy_static::lazy_static;
use rand::{thread_rng, Rng};

lazy_static! {
    static ref ERROR_LEVELS: Vec<&'static str> =
        vec!["alert", "crit", "debug", "emerg", "error", "info", "notice", "trace1-8", "warn",];
    static ref HTTP_CODES: Vec<usize> =
        vec![200, 300, 301, 302, 304, 307, 400, 401, 403, 404, 410, 500, 501, 503, 550,];
    static ref HTTP_ENDPOINTS: Vec<&'static str> = vec!["/foo", "/bar"];
    static ref HTTP_VERSIONS: Vec<&'static str> = vec!["HTTP/1.0", "HTTP/1.1", "HTTP/2.0"];
    static ref ERROR_MESSAGES: Vec<&'static str> = vec!["something went wrong", "oops"];
    static ref APACHE_COMMON_TIME_FORMAT: &'static str = "%d/%b/%Y:%T %z";
    static ref APACHE_ERROR_TIME_FORMAT: &'static str = "%a %b %d %T %Y";
    static ref SYSLOG_TIME_FORMAT: &'static str = "%+";
}

pub fn apache_common_log_line() -> String {
    // Example log line:
    // 173.159.239.159 - schoen1464 [31/Oct/2020:19:06:10 -0700] "POST /wireless HTTP/2.0" 100 20815
    format!(
        "{} - {} [{}] \"{} {} {}\" {} {}",
        ipv4_address(),
        username(),
        timestamp_apache_common(),
        http_method(),
        http_endpoint(),
        http_version(),
        http_code(),
        byte_size(),
    )
}

pub fn apache_error_log_line() -> String {
    // Example log line:
    // [Sat Oct 31 19:27:55 2020] [deleniti:crit] [pid 879:tid 9607] [client 169.198.228.174:1364] Something bad happened
    format!(
        "[{}] [{}:{}] [pid {}:tid] [client {}:{}] {}",
        timestamp_apache_error(),
        username(),
        error_level(),
        pid(),
        ipv4_address(),
        port(),
        error_message(),
    )
}

pub fn syslog_log_line() -> String {
    // Example log line:
    // <65>2 2020-11-05T18:11:43.975Z chiefubiquitous.io totam 6899 ID44 - Something bad happened
    format!(
        "<{}>{} {} {} {} {} ID{} - {}",
        prival(),
        syslog_version(),
        timestamp_syslog(),
        domain(),
        username(),
        random_in_range(100, 9999),
        random_in_range(1, 999),
        error_message(),
    )
}

// Formatted timestamps
fn timestamp_apache_common() -> String {
    Local::now().format(&APACHE_COMMON_TIME_FORMAT).to_string()
}

fn timestamp_apache_error() -> String {
    Local::now().format(&APACHE_ERROR_TIME_FORMAT).to_string()
}

fn timestamp_syslog() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

// Other random strings
fn domain() -> String {
    gen_domain()
}

fn error_level() -> String {
    random_from_vec(ERROR_LEVELS.to_vec()).to_string()
}

fn error_message() -> String {
    random_from_vec(ERROR_MESSAGES.to_vec()).to_string()
}

fn http_code() -> String {
    random_from_vec(HTTP_CODES.to_vec()).to_string()
}

fn byte_size() -> String {
    random_in_range(50, 50000)
}

fn http_endpoint() -> String {
    random_from_vec(HTTP_ENDPOINTS.to_vec()).into()
}

fn http_method() -> String {
    gen_http_method()
}

fn http_version() -> String {
    random_from_vec(HTTP_VERSIONS.to_vec()).into()
}

fn ipv4_address() -> String {
    gen_ipv4()
}

fn pid() -> String {
    random_in_range(1, 9999)
}

fn port() -> String {
    random_in_range(1024, 65535)
}

fn prival() -> String {
    random_in_range(0, 191)
}

fn username() -> String {
    gen_username()
}

fn syslog_version() -> String {
    random_in_range(1, 3)
}

// Helper functions
fn random_in_range(min: usize, max: usize) -> String {
    thread_rng().gen_range(min, max).to_string()
}

fn random_from_vec<T: Copy>(v: Vec<T>) -> T {
    v[thread_rng().gen_range(0, v.len())]
}
