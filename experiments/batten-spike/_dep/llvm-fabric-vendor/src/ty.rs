//! Types and constant values. Small on purpose: i1/i32/i64/f64.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Type {
    I1,
    I32,
    I64,
    F64,
}

impl Type {
    pub fn name(self) -> &'static str {
        match self {
            Type::I1 => "i1",
            Type::I32 => "i32",
            Type::I64 => "i64",
            Type::F64 => "f64",
        }
    }

    pub fn parse(s: &str) -> Option<Type> {
        match s {
            "i1" => Some(Type::I1),
            "i32" => Some(Type::I32),
            "i64" => Some(Type::I64),
            "f64" => Some(Type::F64),
            _ => None,
        }
    }
}

/// A constant value. The variant must agree with the declared type
/// (checked by the verifier, code V11).
///
/// Note: PartialEq on F64 means `0.0 == -0.0` and NaN never equals itself.
/// The parser rejects NaN literals and folding skips NaN-producing folds,
/// so within verified fabrics equality is well behaved. Bit-level float
/// identity is NOT claimed — structural equality is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ConstVal {
    I1(bool),
    I32(i32),
    I64(i64),
    F64(f64),
}

impl ConstVal {
    pub fn ty(&self) -> Type {
        match self {
            ConstVal::I1(_) => Type::I1,
            ConstVal::I32(_) => Type::I32,
            ConstVal::I64(_) => Type::I64,
            ConstVal::F64(_) => Type::F64,
        }
    }

    /// Render for the textual format. Round-trips through `parse`.
    pub fn render(&self) -> String {
        match self {
            ConstVal::I1(b) => b.to_string(),
            ConstVal::I32(v) => v.to_string(),
            ConstVal::I64(v) => format!("{}i64", v),
            ConstVal::F64(v) => format!("{:?}", v),
        }
    }

    pub fn parse(ty: Type, s: &str) -> Option<ConstVal> {
        match ty {
            Type::I1 => match s {
                "true" => Some(ConstVal::I1(true)),
                "false" => Some(ConstVal::I1(false)),
                _ => None,
            },
            Type::I32 => s.parse::<i32>().ok().map(ConstVal::I32),
            Type::I64 => match s.strip_suffix("i64") {
                Some(n) => n.parse::<i64>().ok().map(ConstVal::I64),
                None => None,
            },
            Type::F64 => match s {
                "inf" => Some(ConstVal::F64(f64::INFINITY)),
                "-inf" => Some(ConstVal::F64(f64::NEG_INFINITY)),
                "NaN" | "nan" | "-nan" => None, // rejected on purpose (v0)
                _ => {
                    let v = s.parse::<f64>().ok()?;
                    if v.is_nan() {
                        None
                    } else {
                        Some(ConstVal::F64(v))
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_roundtrip_all_types() {
        for (ty, text) in [
            (Type::I1, "true"),
            (Type::I1, "false"),
            (Type::I32, "-17"),
            (Type::I64, "9007199254740993i64"),
            (Type::F64, "2.5"),
            (Type::F64, "-0.0"),
            (Type::F64, "inf"),
        ] {
            let v = ConstVal::parse(ty, text).unwrap_or_else(|| panic!("parse {} {}", ty.name(), text));
            assert_eq!(v.ty(), ty, "type of {} must be {}", text, ty.name());
            let rendered = v.render();
            let again = ConstVal::parse(ty, &rendered)
                .unwrap_or_else(|| panic!("reparse {} -> {}", text, rendered));
            assert_eq!(v, again, "render/parse roundtrip for {}", text);
        }
    }

    #[test]
    fn nan_literals_rejected() {
        assert!(ConstVal::parse(Type::F64, "NaN").is_none());
        assert!(ConstVal::parse(Type::F64, "nan").is_none());
    }
}
