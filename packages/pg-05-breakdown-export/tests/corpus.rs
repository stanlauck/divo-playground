// SPDX-License-Identifier: MIT OR Apache-2.0
use pg_05_breakdown_export::{from_json, to_csv, to_fdx};
#[test]
fn synthetic_exports_reproduce_exactly_including_csv_crlf() {
    let value = from_json(include_bytes!("../samples/synthetic.json").as_slice()).unwrap();
    assert_eq!(
        to_fdx(&value).unwrap().as_bytes(),
        include_bytes!("../samples/synthetic.fdx")
    );
    assert_eq!(
        to_csv(&value).unwrap().as_bytes(),
        include_bytes!("../samples/synthetic.csv")
    );
}
