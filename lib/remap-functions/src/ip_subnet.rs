use lazy_static::lazy_static;
use regex::Regex;
use remap::prelude::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

lazy_static! {
    static ref RE: Regex = Regex::new(r"/(?P<subnet>\d*)").unwrap();
}

#[derive(Clone, Copy, Debug)]
pub struct IpSubnet;

impl Function for IpSubnet {
    fn identifier(&self) -> &'static str {
        "ip_subnet"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "subnet",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let subnet = arguments.required("subnet")?.boxed();

        Ok(Box::new(IpSubnetFn { value, subnet }))
    }
}

#[derive(Debug, Clone)]
struct IpSubnetFn {
    value: Box<dyn Expression>,
    subnet: Box<dyn Expression>,
}

impl IpSubnetFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, subnet: Box<dyn Expression>) -> Self {
        Self { value, subnet }
    }
}

impl Expression for IpSubnetFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value: IpAddr = self
            .value
            .execute(state, object)?
            .try_bytes_utf8_lossy()?
            .parse()
            .map_err(|err| format!("unable to parse IP address: {}", err))?;

        let mask = self.subnet.execute(state, object)?;
        let mask = mask.try_bytes_utf8_lossy()?;

        let mask = if mask.starts_with('/') {
            // The parameter is a subnet.
            let subnet = parse_subnet(&mask)?;
            match value {
                IpAddr::V4(_) => {
                    if subnet > 32 {
                        return Err("subnet cannot be greater than 32 for ipv4 addresses".into());
                    }

                    ipv4_mask(subnet)
                }
                IpAddr::V6(_) => {
                    if subnet > 128 {
                        return Err("subnet cannot be greater than 128 for ipv6 addresses".into());
                    }

                    ipv6_mask(subnet)
                }
            }
        } else {
            // The parameter is a mask.
            mask.parse()
                .map_err(|err| format!("unable to parse mask: {}", err))?
        };

        Ok(mask_ips(value, mask)?.to_string().into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .merge(self.subnet.type_def(state).into_fallible(true))
            .into_fallible(true)
            .with_constraint(value::Kind::Bytes)
    }
}

/// Parses a subnet in the form "/8" returns the number.
fn parse_subnet(subnet: &str) -> Result<u32> {
    let subnet = RE
        .captures(subnet)
        .ok_or_else(|| format!("{} is not a valid subnet", subnet))?;

    let subnet = subnet["subnet"].parse().expect("digits ensured by regex");

    Ok(subnet)
}

/// Masks the address by performing a bitwise AND between the two addresses.
fn mask_ips(ip: IpAddr, mask: IpAddr) -> Result<IpAddr> {
    match (ip, mask) {
        (IpAddr::V4(addr), IpAddr::V4(mask)) => {
            let addr: u32 = addr.into();
            let mask: u32 = mask.into();
            Ok(Ipv4Addr::from(addr & mask).into())
        }
        (IpAddr::V6(addr), IpAddr::V6(mask)) => {
            let mut masked = [0; 8];
            for ((masked, addr), mask) in masked
                .iter_mut()
                .zip(addr.segments().iter())
                .zip(mask.segments().iter())
            {
                *masked = addr & mask
            }

            Ok(IpAddr::from(masked))
        }
        (IpAddr::V6(_), IpAddr::V4(_)) => {
            Err("attempting to mask an ipv6 address with an ipv4 mask".into())
        }
        (IpAddr::V4(_), IpAddr::V6(_)) => {
            Err("attempting to mask an ipv4 address with an ipv6 mask".into())
        }
    }
}

/// Returns an ipv4 address that masks out the given number of bits.
fn ipv4_mask(subnet_bits: u32) -> IpAddr {
    let bits = !0u32 << (32 - subnet_bits);
    Ipv4Addr::from(bits).into()
}

/// Returns an ipv6 address that masks out the given number of bits.
fn ipv6_mask(subnet_bits: u32) -> IpAddr {
    let bits = !0u128 << (128 - subnet_bits);
    Ipv6Addr::from(bits).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    remap::test_type_def![value_string {
        expr: |_| IpSubnetFn {
            value: Literal::from("192.168.0.1").boxed(),
            subnet: Literal::from("/1").boxed(),
        },
        def: TypeDef {
            kind: value::Kind::Bytes,
            fallible: true,
            ..Default::default()
        },
    }];

    #[test]
    fn ip_subnet() {
        let cases = vec![
            (
                btreemap! { "foo" => "192.168.10.23" },
                Ok(Value::from("192.168.0.0")),
                IpSubnetFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from("255.255.0.0")),
                ),
            ),
            (
                btreemap! { "foo" => "2404:6800:4003:c02::64" },
                Ok(Value::from("2400::")),
                IpSubnetFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from("ff00::")),
                ),
            ),
            (
                btreemap! { "foo" => "192.168.10.23" },
                Ok(Value::from("192.168.0.0")),
                IpSubnetFn::new(Box::new(Path::from("foo")), Box::new(Literal::from("/16"))),
            ),
            (
                btreemap! { "foo" => "192.168.10.23" },
                Ok(Value::from("192.160.0.0")),
                IpSubnetFn::new(Box::new(Path::from("foo")), Box::new(Literal::from("/12"))),
            ),
            (
                btreemap! { "foo" => "2404:6800:4003:c02::64" },
                Ok(Value::from("2404:6800::")),
                IpSubnetFn::new(Box::new(Path::from("foo")), Box::new(Literal::from("/32"))),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object = Value::Map(object);
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
