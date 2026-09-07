use xp_wire::registers::{coll_byte, coll_byte_utf8, minted_token_of, parse_eip4, Eip4};
use xp_wire::{decode_block, DecodedTx};

fn fixture(h: u32) -> String {
    std::fs::read_to_string(format!(
        "{}/../../tests/fixtures/blocks/{h}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn coll_byte_decodes() {
    assert_eq!(coll_byte("0e0568656c6c6f"), Some(b"hello".to_vec()));
    assert_eq!(coll_byte("0e05ab"), None);
    assert_eq!(coll_byte("0500"), None);
    assert_eq!(coll_byte("0e0568656c6c6f00"), None);
}

#[test]
fn coll_byte_utf8_decodes() {
    assert_eq!(coll_byte_utf8("0e0568656c6c6f"), Some("hello".to_string()));
    // invalid UTF-8 bytes -> None
    assert_eq!(coll_byte_utf8("0e02ffff"), None);
    // not a valid Coll[Byte] at all -> None
    assert_eq!(coll_byte_utf8("0500"), None);
}

#[test]
fn eip4_from_registers() {
    // R5's "0e04" + "what" (4 bytes) — the brief's literal text has "0e05" here, but that
    // declares a 5-byte payload against 4 actual bytes, which `coll_byte`'s own exact-
    // consumption rule (see `coll_byte_decodes` above) rejects; "0e04" is the consistent
    // encoding of the same 4-byte string and is what the expected `description` requires.
    let regs = r#"{"R4":"0e0568656c6c6f","R5":"0e0477686174","R6":"0e0132","R7":"0e020101"}"#;
    assert_eq!(
        parse_eip4(regs),
        Eip4 {
            name: "hello".into(),
            description: "what".into(),
            decimals: Some(2),
            token_type: Some("0101".into())
        }
    );
    assert_eq!(parse_eip4("{}"), Eip4::default());
    assert_eq!(parse_eip4(r#"{"R6":"0e02ffff"}"#).decimals, None);
}

#[test]
fn eip4_missing_registers_default_empty() {
    let e = parse_eip4(r#"{"R4":"0e0568656c6c6f"}"#);
    assert_eq!(e.name, "hello");
    assert_eq!(e.description, "");
    assert_eq!(e.decimals, None);
    assert_eq!(e.token_type, None);
}

#[test]
fn minted_token_on_sigusd_mint_fixture() {
    // Block 453051 contains the SigUSD mint tx 695f7249...415f9: the token id equals the
    // id of the transaction's first input box, and R4/R5/R6 carry the EIP-4 metadata.
    let b = decode_block(&fixture(453051)).unwrap();
    let mint_tx: &DecodedTx = b
        .txs
        .iter()
        .find(|t| {
            hex::encode(t.id.0)
                == "695f7249f70e4d7ec6239694e1cc720f3e37c217bd54fbad74e7fdfbaf9415f9"
        })
        .expect("mint tx present in fixture");

    let first_input = &mint_tx.inputs[0];
    let minted = minted_token_of(&first_input.0, &mint_tx.outputs).expect("mint detected");
    assert_eq!(hex::encode(minted.0), hex::encode(first_input.0));
    assert_eq!(minted.1, 10_000_000_000_001);

    let eip4 = parse_eip4(&mint_tx.outputs[0].registers_json);
    assert_eq!(eip4.name, "SigUSD");
    assert_eq!(eip4.description, "SigmaUSD - V2");
    assert_eq!(eip4.decimals, Some(2));
}

#[test]
fn minted_token_of_none_when_no_mint() {
    let b = decode_block(&fixture(1866000)).unwrap();
    for tx in &b.txs {
        if tx.inputs.is_empty() {
            continue;
        }
        assert_eq!(minted_token_of(&tx.inputs[0].0, &tx.outputs), None);
    }
}
