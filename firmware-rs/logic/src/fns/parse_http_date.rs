// @fn parse_http_date
/// IMF-fixdate HTTP Date header to unix seconds.
/// Contract: the result equals `spec_http_date(s)`. No panic for any input.
pub fn parse_http_date(s: &[u8]) -> Option<u64> {
    // @body
    if s.len() != 29 {
        return None;
    }
    if s[3] != b',' || s[4] != b' ' || s[7] != b' ' || s[11] != b' ' || s[16] != b' ' || s[19] != b':' || s[22] != b':' || s[25] != b' ' || s[26] != b'G' || s[27] != b'M' || s[28] != b'T' {
        return None;
    }
    let dig = |i: usize| -> Option<u64> {
        let a = s[i];
        let b = s[i + 1];
        if a.is_ascii_digit() && b.is_ascii_digit() {
            Some((a - b'0') as u64 * 10 + (b - b'0') as u64)
        } else {
            None
        }
    };
    let day = dig(5)?;
    let mon: u64 = match &s[8..11] {
        b"Jan" => 1,
        b"Feb" => 2,
        b"Mar" => 3,
        b"Apr" => 4,
        b"May" => 5,
        b"Jun" => 6,
        b"Jul" => 7,
        b"Aug" => 8,
        b"Sep" => 9,
        b"Oct" => 10,
        b"Nov" => 11,
        b"Dec" => 12,
        _ => return None,
    };
    let year = dig(12)? * 100 + dig(14)?;
    let hh = dig(17)?;
    let mm = dig(20)?;
    let ss = dig(23)?;
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let dim: u64 = match mon {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if leap {
                29
            } else {
                28
            }
        }
    };
    if year < 1970 || day < 1 || day > dim || hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    let y = if mon <= 2 { year - 1 } else { year };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = (mon + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3_600 + mm * 60 + ss)
}
// @end

// @harness contract_parse_http_date target=parse_http_date stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(32)]
fn contract_parse_http_date() {
    let s_buf: [u8; 30] = kani::any();
    let s_len: usize = kani::any();
    kani::assume(s_len <= 30);
    let s: &[u8] = &s_buf[..s_len];
    let want = spec_http_date(s);
    let got = parse_http_date(s);
    assert!(got == want);
}
// @end
