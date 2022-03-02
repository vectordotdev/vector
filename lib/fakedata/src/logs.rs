use chrono::{
    format::{DelayedFormat, StrftimeItems},
    prelude::Local,
    SecondsFormat,
};
use fakedata_generator::{gen_domain, gen_ipv4, gen_username};
use rand::{thread_rng, Rng};

static APPLICATION_NAMES: [&str; 10] = [
    "auth", "data", "deploy", "etl", "scraper", "cron", "ingress", "egress", "alerter", "fwd",
];

static ERROR_LEVELS: [&str; 9] = [
    "alert", "crit", "debug", "emerg", "error", "info", "notice", "trace1-8", "warn",
];

static HTTP_CODES: [usize; 15] = [
    200, 300, 301, 302, 304, 307, 400, 401, 403, 404, 410, 500, 501, 503, 550,
];

static HTTP_VERSIONS: [&str; 3] = ["HTTP/1.0", "HTTP/1.1", "HTTP/2.0"];
static HTTP_METHODS: [&str; 7] = ["DELETE", "GET", "HEAD", "OPTION", "PATCH", "POST", "PUT"];

static HTTP_ENDPOINTS: [&str; 9] = [
    "/wp-admin",
    "/controller/setup",
    "/user/booperbot124",
    "/apps/deploy",
    "/observability/metrics/production",
    "/secret-info/open-sesame",
    "/booper/bopper/mooper/mopper",
    "/do-not-access/needs-work",
    "/this/endpoint/prints/money",
];

static ERROR_MESSAGES: [&str; 9] = [
    "There's a breach in the warp core, captain",
    "Great Scott! We're never gonna reach 88 mph with the flux capacitor in its current state!",
    "You're not gonna believe what just happened",
    "#hugops to everyone who has to deal with this",
    "Take a breath, let it go, walk away",
    "A bug was encountered but not in Vector, which doesn't have bugs",
    "We're gonna need a bigger boat",
    "Maybe we just shouldn't use computers",
    "Pretty pretty pretty good",
];

const APACHE_COMMON_TIME_FORMAT: &str = "%d/%b/%Y:%T %z";
const APACHE_ERROR_TIME_FORMAT: &str = "%a %b %d %T %Y";
const SYSLOG_3164_FORMAT: &str = "%b %d %T";
const JSON_TIME_FORMAT: &str = "%d/%b/%Y:%T";

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

pub fn syslog_3164_log_line() -> String {
    format!(
        "<{}>{} {} {}[{}]: {}",
        priority(),
        timestamp_syslog_3164(),
        domain(),
        application(),
        pid(),
        error_message()
    )
}

pub fn syslog_5424_log_line() -> String {
    // Example log line:
    // <65>2 2020-11-05T18:11:43.975Z chiefubiquitous.io totam 6899 ID44 - Something bad happened
    format!(
        "<{}>{} {} {} {} {} ID{} - {}",
        priority(),
        syslog_version(),
        timestamp_syslog_5424(),
        domain(),
        username(),
        random_in_range(100, 9999),
        random_in_range(1, 999),
        error_message(),
    )
}

pub fn json_log_line() -> String {
    // Borrowed from Flog: https://github.com/mingrammer/flog/blob/master/log.go#L24
    // Example log line:
    // {"host":"208.171.64.160", "user-identifier":"hoppe7055", "datetime":" -0800", "method": \
    //   "PATCH", "request": "/web+services/cross-media/strategize", "protocol":"HTTP/1.1", \
    //   "status":403, "bytes":25926, "referer": "https://www.leadworld-class.org/revolutionize/applications"}
    format!(
        "{{\"host\":\"{}\",\"user-identifier\":\"{}\",\"datetime\":\"{}\",\"method\":\"{}\",\"request\":\"{}\",\"protocol\":\"{}\",\"status\":\"{}\",\"bytes\":{},\"referer\":\"{}\"}}",
        ipv4_address(),
        username(),
        timestamp_json(),
        http_method(),
        http_endpoint(),
        http_version(),
        http_code(),
        random_in_range(1000, 50000),
        referer(),
    )
}

// Formatted timestamps
fn timestamp_apache_common() -> DelayedFormat<StrftimeItems<'static>> {
    Local::now().format(APACHE_COMMON_TIME_FORMAT)
}

fn timestamp_apache_error() -> DelayedFormat<StrftimeItems<'static>> {
    Local::now().format(APACHE_ERROR_TIME_FORMAT)
}

fn timestamp_syslog_3164() -> DelayedFormat<StrftimeItems<'static>> {
    Local::now().format(SYSLOG_3164_FORMAT)
}

fn timestamp_syslog_5424() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn timestamp_json() -> DelayedFormat<StrftimeItems<'static>> {
    Local::now().format(JSON_TIME_FORMAT)
}

// Other random strings
fn application() -> &'static str {
    random_from_array(&APPLICATION_NAMES)
}

fn domain() -> String {
    gen_domain()
}

fn error_level() -> &'static str {
    random_from_array(&ERROR_LEVELS)
}

fn error_message() -> &'static str {
    random_from_array(&ERROR_MESSAGES)
}

fn http_code() -> usize {
    random_from_array_copied(&HTTP_CODES)
}

fn byte_size() -> usize {
    random_in_range(50, 50000)
}

fn http_endpoint() -> &'static str {
    random_from_array(&HTTP_ENDPOINTS)
}

fn http_method() -> &'static str {
    random_from_array(&HTTP_METHODS)
}

fn http_version() -> &'static str {
    random_from_array(&HTTP_VERSIONS)
}

fn ipv4_address() -> String {
    gen_ipv4()
}

fn pid() -> usize {
    random_in_range(1, 9999)
}

fn port() -> usize {
    random_in_range(1024, 65535)
}

fn priority() -> usize {
    random_in_range(0, 191)
}

fn referer() -> String {
    format!("https://{}{}", domain(), http_endpoint())
}

fn username() -> String {
    gen_username()
}

fn syslog_version() -> usize {
    random_in_range(1, 3)
}

// Helper functions
fn random_in_range(min: usize, max: usize) -> usize {
    thread_rng().gen_range(min..max)
}

fn random_from_array<T: ?Sized>(v: &'static [&'static T]) -> &'static T {
    v[thread_rng().gen_range(0..v.len())]
}

fn random_from_array_copied<T: Copy>(v: &[T]) -> T {
    v[thread_rng().gen_range(0..v.len())]
}
