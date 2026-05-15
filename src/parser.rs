use heapless::{String, Vec};

const EXPECTED_DEPARTURE_KEY: &str = "\"ExpectedDepartureTime\":\"";
const RESPONSE_TIMESTAMP_KEY: &str = "\"ResponseTimestamp\":\"";

pub fn extract_expected_departures(json: &str) -> Vec<String<32>, 16> {
    let mut departures: Vec<String<32>, 16> = Vec::new();
    let mut cursor = 0usize;

    while cursor < json.len() {
        let Some(key_pos) = json[cursor..].find(EXPECTED_DEPARTURE_KEY) else {
            break;
        };

        let value_start = cursor + key_pos + EXPECTED_DEPARTURE_KEY.len();
        let Some(value_end_rel) = json[value_start..].find('"') else {
            break;
        };

        let value_end = value_start + value_end_rel;
        let raw = &json[value_start..value_end];

        let mut iso = String::<32>::new();
        let _ = iso.push_str(raw);
        let _ = departures.push(iso);

        cursor = value_end + 1;
    }

    departures
}

pub fn to_hhmm(iso_ts: &str) -> Option<String<8>> {
    if iso_ts.len() < 16 {
        return None;
    }

    let hhmm = &iso_ts[11..16];
    let mut out = String::<8>::new();
    let _ = out.push_str(hhmm);
    Some(out)
}

pub fn extract_response_timestamp(json: &str) -> Option<String<32>> {
    let key_pos = json.find(RESPONSE_TIMESTAMP_KEY)?;
    let value_start = key_pos + RESPONSE_TIMESTAMP_KEY.len();
    let value_end_rel = json[value_start..].find('"')?;
    let value_end = value_start + value_end_rel;
    let raw = &json[value_start..value_end];

    let mut iso = String::<32>::new();
    let _ = iso.push_str(raw);
    Some(iso)
}

pub fn minutes_until(from_iso: &str, to_iso: &str) -> Option<i32> {
    let from_s = iso_to_epoch_seconds(from_iso)?;
    let to_s = iso_to_epoch_seconds(to_iso)?;
    let diff = to_s - from_s;

    if diff >= 0 {
        Some(((diff + 59) / 60) as i32)
    } else {
        Some((diff / 60) as i32)
    }
}

fn iso_to_epoch_seconds(iso: &str) -> Option<i64> {
    if iso.len() < 19 {
        return None;
    }

    if iso.as_bytes().get(4) != Some(&b'-')
        || iso.as_bytes().get(7) != Some(&b'-')
        || iso.as_bytes().get(10) != Some(&b'T')
        || iso.as_bytes().get(13) != Some(&b':')
        || iso.as_bytes().get(16) != Some(&b':')
    {
        return None;
    }

    let year = parse_i64(&iso[0..4])?;
    let month = parse_i64(&iso[5..7])?;
    let day = parse_i64(&iso[8..10])?;
    let hour = parse_i64(&iso[11..13])?;
    let minute = parse_i64(&iso[14..16])?;
    let second = parse_i64(&iso[17..19])?;

    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn parse_i64(s: &str) -> Option<i64> {
    let mut value = 0i64;
    for &b in s.as_bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value * 10 + (b - b'0') as i64;
    }
    Some(value)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y_adj = y - if m <= 2 { 1 } else { 0 };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
