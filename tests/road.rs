use route::road::Road;

fn binary() -> Vec<u8> {
    let mut b = Vec::new();
    for v in [2u64, 1] {
        b.extend(v.to_le_bytes());
    }
    for v in [36.0f64, 36.1, 127.0, 127.1] {
        b.extend(v.to_le_bytes());
    }
    for v in [0u32, 1] {
        b.extend(v.to_le_bytes());
    }
    b.extend(7.5f64.to_le_bytes());
    b
}

#[test]
fn validates_every_binary_entry_path() {
    let b = binary();
    let r = Road::decode(&b).unwrap();
    assert_eq!(r.g.arcs(0).collect::<Vec<_>>(), vec![(1, 7.5)]);
    assert_eq!(r.xy.len(), 2);
    for end in 0..b.len() {
        assert!(Road::decode(&b[..end]).is_err());
    }
    let mut bad = b.clone();
    bad.push(0);
    assert!(Road::decode(&bad).is_err());
    for (at, data) in [
        (16, f64::NAN.to_le_bytes().to_vec()),
        (48, 2u32.to_le_bytes().to_vec()),
        (56, (-1.0f64).to_le_bytes().to_vec()),
        (56, f64::INFINITY.to_le_bytes().to_vec()),
    ] {
        let mut bad = b.clone();
        bad[at..at + data.len()].copy_from_slice(&data);
        assert!(Road::decode(&bad).is_err(), "offset {at}");
    }
    let mut bad = b;
    bad[..8].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(Road::decode(&bad).is_err());
}
