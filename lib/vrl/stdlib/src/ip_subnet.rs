use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use lazy_static::lazy_static;
use regex::Regex;
use vrl::prelude::*;

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
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "subnet",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "subnet",
            source: r#"ip_subnet!("192.168.0.1", "/1")"#,
            result: Ok("128.0.0.0"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let subnet = arguments.required("subnet");

        Ok(Box::new(IpSubnetFn { value, subnet }))
    }
}

#[derive(Debug, Clone)]
struct IpSubnetFn {
    value: Box<dyn Expression>,
    subnet: Box<dyn Expression>,
}

impl Expression for IpSubnetFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value: IpAddr = self
            .value
            .resolve(ctx)?
            .try_bytes_utf8_lossy()?
            .parse()
            .map_err(|err| format!("unable to parse IP address: {}", err))?;

        let mask = self.subnet.resolve(ctx)?;
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

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
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

    test_function![
        ip_subnet => IpSubnet;

        ipv4 {
            args: func_args![value: "192.168.10.23",
                             subnet: "255.255.0.0"],
            want: Ok(value!("192.168.0.0")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        ipv6 {
            args: func_args![value: "2404:6800:4003:c02::64",
                             subnet: "ff00::"],
            want: Ok(value!("2400::")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        ipv4_subnet {
            args: func_args![value: "192.168.10.23",
                             subnet: "/16"],
            want: Ok(value!("192.168.0.0")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        ipv4_smaller_subnet {
            args: func_args![value: "192.168.10.23",
                             subnet: "/12"],
            want: Ok(value!("192.160.0.0")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        ipv6_subnet {
            args: func_args![value: "2404:6800:4003:c02::64",
                             subnet: "/32"],
            want: Ok(value!("2404:6800::")),
            tdef: TypeDef::new().fallible().bytes(),
        }
    ];
}
