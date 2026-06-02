use lightning_invoice::Bolt11Invoice;

#[allow(dead_code)]
pub struct ParsedBolt11 {
    pub amount_sats: u64,
    pub payment_hash: bitcoin_hashes::sha256::Hash,
}

#[allow(dead_code)]
pub fn parse_bolt11(invoice: &str) -> Result<ParsedBolt11, String> {
    let parsed: Bolt11Invoice = invoice
        .parse()
        .map_err(|e| format!("Invalid bolt11 invoice: {}", e))?;
    let amount_msats = parsed
        .amount_milli_satoshis()
        .ok_or("Invoice has no amount")?;
    Ok(ParsedBolt11 {
        amount_sats: amount_msats.div_ceil(1000),
        payment_hash: *parsed.payment_hash(),
    })
}

pub fn parse_bolt11_amount(bolt11: &str) -> Option<u64> {
    let parsed: Bolt11Invoice = bolt11.parse().ok()?;
    parsed.amount_milli_satoshis().map(|msats| msats.div_ceil(1000))
}

pub fn parse_bolt11_amount_msats(bolt11: &str) -> Option<u64> {
    let parsed: Bolt11Invoice = bolt11.parse().ok()?;
    parsed.amount_milli_satoshis()
}
