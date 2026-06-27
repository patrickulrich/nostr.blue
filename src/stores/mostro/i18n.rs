//! Mostro P2P scoped i18n.
//!
//! Phase 12: provides a `mostro_tr!()` macro for P2P-specific strings.
//! Backed by a compile-time string table keyed by `(namespace.key, Locale)`.
//! Falls back to English when the active locale has no entry for a key,
//! and to the key itself when no entry exists at all.
//!
//! Currently supports: English (default), Spanish, Italian, German, French.
//! Additional locales can be added by extending `build_strings()`.
//!
//! This is intentionally scoped to P2P — the rest of the app continues
//! to use hard-coded English. A future app-wide i18n effort can migrate
//! these keys to the broader system.

use dioxus::prelude::*;
use std::collections::HashMap;

/// Supported locales for Mostro P2P strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Locale {
    En,
    Es,
    It,
    De,
    Fr,
}

impl Locale {
    /// Parse a locale string (e.g., "en", "es", "it", "de").
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "es" => Locale::Es,
            "it" => Locale::It,
            "de" => Locale::De,
            "fr" => Locale::Fr,
            _ => Locale::En,
        }
    }

    /// Check if a language code matches this locale.
    #[allow(dead_code)]
    pub fn matches(&self, lang: &str) -> bool {
        let lang = lang.to_lowercase();
        matches!(
            (self, lang.as_str()),
            (Locale::En, "en") | (Locale::Es, "es") | (Locale::It, "it") | (Locale::De, "de")
                | (Locale::Fr, "fr")
        )
    }
}

/// Global active locale. Defaults to English.
/// Phase 12.3: wired to the browser's `navigator.language` on startup.
pub static ACTIVE_LOCALE: GlobalSignal<Locale> = Signal::global(|| Locale::En);

/// Build the compile-time string table.
fn build_strings() -> HashMap<(&'static str, Locale), &'static str> {
    let mut m = HashMap::new();

    // --- Trade action panel ---
    m.insert(("mostro.actions", Locale::En), "Actions");
    m.insert(("mostro.payout_invoice", Locale::En), "Payout Invoice");
    m.insert(("mostro.submit_invoice", Locale::En), "Submit Invoice");
    m.insert(("mostro.generate_nwc", Locale::En), "Generate (NWC)");
    m.insert(("mostro.fiat_sent", Locale::En), "Mark Fiat Sent");
    m.insert(("mostro.release_sats", Locale::En), "Release Sats");
    m.insert(("mostro.cancel_trade", Locale::En), "Cancel Trade");
    m.insert(("mostro.accept_cancel", Locale::En), "Accept Cancel");
    m.insert(("mostro.open_dispute", Locale::En), "Open Dispute");
    m.insert(("mostro.rate_counterpart", Locale::En), "Rate Counterpart");
    m.insert(("mostro.submit_rating", Locale::En), "Submit Rating");
    m.insert(("mostro.pay_bond", Locale::En), "Pay Bond");
    m.insert(("mostro.claim_bond", Locale::En), "Claim Bond Payout");

    // --- Take button ---
    m.insert(("mostro.take_with_mostro", Locale::En), "Take with Mostro →");
    m.insert(("mostro.taking", Locale::En), "Taking…");
    m.insert(("mostro.specify_amount", Locale::En), "Specify Amount");
    m.insert(("mostro.take_order", Locale::En), "Take Order");
    m.insert(("mostro.payout_invoice_optional", Locale::En), "Payout Invoice (optional)");
    m.insert(("mostro.skip", Locale::En), "Skip");

    // --- Confirm modals ---
    m.insert(("mostro.confirm_release_title", Locale::En), "Release Sats");
    m.insert(("mostro.confirm_release_body", Locale::En),
        "Releasing settles the Lightning escrow and pays out the buyer. \
         This action is irreversible — only release after you have confirmed \
         receipt of the fiat payment.");
    m.insert(("mostro.release", Locale::En), "Release");
    m.insert(("mostro.dont_release", Locale::En), "Don't Release");
    m.insert(("mostro.confirm_cancel_title", Locale::En), "Cancel Trade");
    m.insert(("mostro.confirm_cancel_body", Locale::En),
        "Are you sure you want to cancel this trade? The counterparty will be notified.");
    m.insert(("mostro.yes_cancel", Locale::En), "Yes, Cancel");
    m.insert(("mostro.keep_trade", Locale::En), "Keep Trade");

    // --- Daemon switch ---
    m.insert(("mostro.switch_daemon", Locale::En), "Switch Mostro Daemon?");
    m.insert(("mostro.switch_and_take", Locale::En), "Switch & Take");

    // --- Settings ---
    m.insert(("mostro.preferences", Locale::En), "Preferences");
    m.insert(("mostro.default_fiat", Locale::En), "Default Fiat Currency");
    m.insert(("mostro.default_ln_address", Locale::En), "Default Lightning Address");
    m.insert(("mostro.notifications_label", Locale::En), "Notifications");
    m.insert(("mostro.trade_updates", Locale::En), "Trade updates");
    m.insert(("mostro.chat_messages", Locale::En), "Chat messages");
    m.insert(("mostro.dispute_updates", Locale::En), "Dispute updates");
    m.insert(("mostro.sound", Locale::En), "Sound");
    m.insert(("mostro.vibration", Locale::En), "Vibration");
    m.insert(("mostro.history_expiration", Locale::En), "Trade History Expiration");
    m.insert(("mostro.save_preferences", Locale::En), "Save Preferences");
    m.insert(("mostro.enable_notifications", Locale::En), "Enable Notifications");
    m.insert(("mostro.notifications_enabled", Locale::En), "✓ Notifications are enabled");

    // --- Status / timeline ---
    m.insert(("mostro.progress", Locale::En), "Progress");
    m.insert(("mostro.in_dispute", Locale::En), "In Dispute");
    m.insert(("mostro.cancel_pending", Locale::En), "Cancel Pending");
    m.insert(("mostro.canceled", Locale::En), "Canceled");
    m.insert(("mostro.mutual_cancel", Locale::En), "Mutual Cancel");
    m.insert(("mostro.admin_canceled", Locale::En), "Admin Canceled");
    m.insert(("mostro.expired", Locale::En), "Expired");

    // --- Chat ---
    m.insert(("mostro.chat", Locale::En), "Chat");
    m.insert(("mostro.chat_locked", Locale::En),
        "Chat will be available once the trade is active and counterparty is revealed.");
    m.insert(("mostro.no_messages", Locale::En), "No messages yet");
    m.insert(("mostro.download", Locale::En), "Download");
    m.insert(("mostro.downloading", Locale::En), "Downloading…");
    m.insert(("mostro.download_encrypted", Locale::En), "Download (encrypted)");

    // --- Reputation ---
    m.insert(("mostro.no_reputation", Locale::En), "No reputation data");
    m.insert(("mostro.counterparty", Locale::En), "Counterparty");

    // --- Rate received toast ---
    m.insert(("mostro.rate_received_title", Locale::En), "Rating received");
    m.insert(("mostro.rate_received_body", Locale::En),
        "You received a {stars}-star rating from your counterparty.");

    // --- Spanish ---
    m.insert(("mostro.actions", Locale::Es), "Acciones");
    m.insert(("mostro.payout_invoice", Locale::Es), "Factura de Pago");
    m.insert(("mostro.submit_invoice", Locale::Es), "Enviar Factura");
    m.insert(("mostro.release_sats", Locale::Es), "Liberar Sats");
    m.insert(("mostro.cancel_trade", Locale::Es), "Cancelar Intercambio");
    m.insert(("mostro.open_dispute", Locale::Es), "Abrir Disputa");
    m.insert(("mostro.take_with_mostro", Locale::Es), "Tomar con Mostro →");
    m.insert(("mostro.taking", Locale::Es), "Tomando…");
    m.insert(("mostro.take_order", Locale::Es), "Tomar Orden");
    m.insert(("mostro.skip", Locale::Es), "Saltar");
    m.insert(("mostro.preferences", Locale::Es), "Preferencias");
    m.insert(("mostro.default_fiat", Locale::Es), "Moneda Fiat Predeterminada");
    m.insert(("mostro.default_ln_address", Locale::Es), "Dirección Lightning Predeterminada");
    m.insert(("mostro.save_preferences", Locale::Es), "Guardar Preferencias");
    m.insert(("mostro.in_dispute", Locale::Es), "En Disputa");
    m.insert(("mostro.expired", Locale::Es), "Expirado");
    m.insert(("mostro.chat", Locale::Es), "Chat");
    m.insert(("mostro.no_messages", Locale::Es), "Sin mensajes");
    m.insert(("mostro.no_reputation", Locale::Es), "Sin datos de reputación");
    m.insert(("mostro.counterparty", Locale::Es), "Contraparte");
    m.insert(("mostro.rate_received_title", Locale::Es), "Calificación recibida");
    m.insert(("mostro.rate_received_body", Locale::Es),
        "Recibiste una calificación de {stars} estrellas de tu contraparte.");
    // Bug #6 fix: complete i18n coverage for Es.
    m.insert(("mostro.pay_bond", Locale::Es), "Pagar Fianza");
    m.insert(("mostro.claim_bond", Locale::Es), "Reclamar Pago de Fianza");
    m.insert(("mostro.confirm_release_title", Locale::Es), "Liberar Sats");
    m.insert(("mostro.confirm_release_body", Locale::Es),
        "¿Confirmar la liberación de los sats del depósito de garantía a la contraparte?");
    m.insert(("mostro.cancel_pending", Locale::Es), "Cancelación Pendiente");
    m.insert(("mostro.canceled", Locale::Es), "Cancelado");
    m.insert(("mostro.mutual_cancel", Locale::Es), "Cancelación Mutua");
    m.insert(("mostro.admin_canceled", Locale::Es), "Cancelado por Admin");
    m.insert(("mostro.progress", Locale::Es), "Progreso");
    m.insert(("mostro.chat_locked", Locale::Es),
        "El chat se desbloquea cuando ambas partes confirman.");
    m.insert(("mostro.download", Locale::Es), "Descargar");
    m.insert(("mostro.downloading", Locale::Es), "Descargando…");
    m.insert(("mostro.download_encrypted", Locale::Es), "Descargar (cifrado)");
    m.insert(("mostro.release", Locale::Es), "Liberar");
    m.insert(("mostro.fiat_sent", Locale::Es), "Fiat Enviado");
    m.insert(("mostro.accept_cancel", Locale::Es), "Aceptar Cancelación");
    m.insert(("mostro.keep_trade", Locale::Es), "Mantener Intercambio");
    m.insert(("mostro.confirm_cancel_title", Locale::Es), "Cancelar Intercambio");
    m.insert(("mostro.confirm_cancel_body", Locale::Es),
        "¿Solicitar cancelación cooperativa? La contraparte debe aceptar.");
    m.insert(("mostro.yes_cancel", Locale::Es), "Sí, Cancelar");
    m.insert(("mostro.dont_release", Locale::Es), "No Liberar");
    m.insert(("mostro.specify_amount", Locale::Es), "Especificar Cantidad");
    m.insert(("mostro.submit_rating", Locale::Es), "Enviar Calificación");
    m.insert(("mostro.rate_counterpart", Locale::Es), "Calificar Contraparte");

    // --- Italian ---
    m.insert(("mostro.actions", Locale::It), "Azioni");
    m.insert(("mostro.payout_invoice", Locale::It), "Fattura di Pagamento");
    m.insert(("mostro.submit_invoice", Locale::It), "Invia Fattura");
    m.insert(("mostro.release_sats", Locale::It), "Rilascia Sats");
    m.insert(("mostro.cancel_trade", Locale::It), "Annulla Scambio");
    m.insert(("mostro.open_dispute", Locale::It), "Apri Disputa");
    m.insert(("mostro.take_with_mostro", Locale::It), "Prendi con Mostro →");
    m.insert(("mostro.taking", Locale::It), "Prendendo…");
    m.insert(("mostro.take_order", Locale::It), "Prendi Ordine");
    m.insert(("mostro.skip", Locale::It), "Salta");
    m.insert(("mostro.preferences", Locale::It), "Preferenze");
    m.insert(("mostro.save_preferences", Locale::It), "Salva Preferenze");
    m.insert(("mostro.in_dispute", Locale::It), "In Disputa");
    m.insert(("mostro.expired", Locale::It), "Scaduto");
    m.insert(("mostro.chat", Locale::It), "Chat");
    m.insert(("mostro.no_messages", Locale::It), "Nessun messaggio");
    m.insert(("mostro.no_reputation", Locale::It), "Nessun dato di reputazione");
    m.insert(("mostro.rate_received_title", Locale::It), "Valutazione ricevuta");
    m.insert(("mostro.rate_received_body", Locale::It),
        "Hai ricevuto una valutazione di {stars} stelle dalla controparte.");
    // Bug #6 fix: complete i18n coverage for It.
    m.insert(("mostro.pay_bond", Locale::It), "Paga Cauzione");
    m.insert(("mostro.claim_bond", Locale::It), "Richiedi Pagamento Cauzione");
    m.insert(("mostro.confirm_release_title", Locale::It), "Rilascia Sats");
    m.insert(("mostro.confirm_release_body", Locale::It),
        "Confermi il rilascio dei sats della fattura di deposito alla controparte?");
    m.insert(("mostro.cancel_pending", Locale::It), "Cancellazione in Sospeso");
    m.insert(("mostro.canceled", Locale::It), "Annullato");
    m.insert(("mostro.mutual_cancel", Locale::It), "Annullamento Reciproco");
    m.insert(("mostro.admin_canceled", Locale::It), "Annullato dall'Admin");
    m.insert(("mostro.progress", Locale::It), "Avanzamento");
    m.insert(("mostro.chat_locked", Locale::It),
        "La chat si sblocca quando entrambe le parti confermano.");
    m.insert(("mostro.download", Locale::It), "Scarica");
    m.insert(("mostro.downloading", Locale::It), "Scaricamento…");
    m.insert(("mostro.download_encrypted", Locale::It), "Scarica (crittografato)");
    m.insert(("mostro.release", Locale::It), "Rilascia");
    m.insert(("mostro.fiat_sent", Locale::It), "Fiat Inviato");
    m.insert(("mostro.accept_cancel", Locale::It), "Accetta Annullamento");
    m.insert(("mostro.keep_trade", Locale::It), "Mantieni Scambio");
    m.insert(("mostro.confirm_cancel_title", Locale::It), "Annulla Scambio");
    m.insert(("mostro.confirm_cancel_body", Locale::It),
        "Richiedere annullamento cooperativo? La controparte deve accettare.");
    m.insert(("mostro.yes_cancel", Locale::It), "Sì, Annulla");
    m.insert(("mostro.dont_release", Locale::It), "Non Rilasciare");
    m.insert(("mostro.specify_amount", Locale::It), "Specifica Importo");
    m.insert(("mostro.submit_rating", Locale::It), "Invia Valutazione");
    m.insert(("mostro.rate_counterpart", Locale::It), "Valuta Controparte");

    // --- German ---
    m.insert(("mostro.actions", Locale::De), "Aktionen");
    m.insert(("mostro.payout_invoice", Locale::De), "Auszahlungsrechnung");
    m.insert(("mostro.submit_invoice", Locale::De), "Rechnung einreichen");
    m.insert(("mostro.release_sats", Locale::De), "Sats freigeben");
    m.insert(("mostro.cancel_trade", Locale::De), "Handel abbrechen");
    m.insert(("mostro.open_dispute", Locale::De), "Streitfall eröffnen");
    m.insert(("mostro.take_with_mostro", Locale::De), "Mit Mostro nehmen →");
    m.insert(("mostro.taking", Locale::De), "Wird genommen…");
    m.insert(("mostro.take_order", Locale::De), "Bestellung annehmen");
    m.insert(("mostro.skip", Locale::De), "Überspringen");
    m.insert(("mostro.preferences", Locale::De), "Einstellungen");
    m.insert(("mostro.save_preferences", Locale::De), "Einstellungen speichern");
    m.insert(("mostro.in_dispute", Locale::De), "Im Streitfall");
    m.insert(("mostro.expired", Locale::De), "Abgelaufen");
    m.insert(("mostro.chat", Locale::De), "Chat");
    m.insert(("mostro.no_messages", Locale::De), "Keine Nachrichten");
    m.insert(("mostro.no_reputation", Locale::De), "Keine Reputationsdaten");
    m.insert(("mostro.rate_received_title", Locale::De), "Bewertung erhalten");
    m.insert(("mostro.rate_received_body", Locale::De),
        "Du hast eine {stars}-Sterne-Bewertung von der Gegenpartei erhalten.");
    // Bug #6 fix: complete i18n coverage for De.
    m.insert(("mostro.pay_bond", Locale::De), "Kaution zahlen");
    m.insert(("mostro.claim_bond", Locale::De), "Kaution-Auszahlung beantragen");
    m.insert(("mostro.confirm_release_title", Locale::De), "Sats freigeben");
    m.insert(("mostro.confirm_release_body", Locale::De),
        "Freigabe der Sats aus der hinterlegten Rechnung an die Gegenpartei bestätigen?");
    m.insert(("mostro.cancel_pending", Locale::De), "Abbruch ausstehend");
    m.insert(("mostro.canceled", Locale::De), "Abgebrochen");
    m.insert(("mostro.mutual_cancel", Locale::De), "Einvernehmlicher Abbruch");
    m.insert(("mostro.admin_canceled", Locale::De), "Vom Admin abgebrochen");
    m.insert(("mostro.progress", Locale::De), "Fortschritt");
    m.insert(("mostro.chat_locked", Locale::De),
        "Chat wird freigeschaltet, sobald beide Parteien bestätigen.");
    m.insert(("mostro.download", Locale::De), "Herunterladen");
    m.insert(("mostro.downloading", Locale::De), "Wird heruntergeladen…");
    m.insert(("mostro.download_encrypted", Locale::De), "Herunterladen (verschlüsselt)");
    m.insert(("mostro.release", Locale::De), "Freigeben");
    m.insert(("mostro.fiat_sent", Locale::De), "Fiat gesendet");
    m.insert(("mostro.accept_cancel", Locale::De), "Abbruch akzeptieren");
    m.insert(("mostro.keep_trade", Locale::De), "Handel beibehalten");
    m.insert(("mostro.confirm_cancel_title", Locale::De), "Handel abbrechen");
    m.insert(("mostro.confirm_cancel_body", Locale::De),
        "Kooperativen Abbruch anfordern? Die Gegenpartei muss zustimmen.");
    m.insert(("mostro.yes_cancel", Locale::De), "Ja, abbrechen");
    m.insert(("mostro.dont_release", Locale::De), "Nicht freigeben");
    m.insert(("mostro.specify_amount", Locale::De), "Betrag angeben");
    m.insert(("mostro.submit_rating", Locale::De), "Bewertung absenden");
    m.insert(("mostro.rate_counterpart", Locale::De), "Gegenpartei bewerten");

    // --- French ---
    m.insert(("mostro.actions", Locale::Fr), "Actions");
    m.insert(("mostro.payout_invoice", Locale::Fr), "Facture de paiement");
    m.insert(("mostro.submit_invoice", Locale::Fr), "Soumettre la facture");
    m.insert(("mostro.generate_nwc", Locale::Fr), "Générer (NWC)");
    m.insert(("mostro.fiat_sent", Locale::Fr), "Marquer fiat envoyé");
    m.insert(("mostro.release_sats", Locale::Fr), "Libérer les sats");
    m.insert(("mostro.cancel_trade", Locale::Fr), "Annuler l'échange");
    m.insert(("mostro.accept_cancel", Locale::Fr), "Accepter l'annulation");
    m.insert(("mostro.open_dispute", Locale::Fr), "Ouvrir un litige");
    m.insert(("mostro.rate_counterpart", Locale::Fr), "Évaluer la contrepartie");
    m.insert(("mostro.submit_rating", Locale::Fr), "Soumettre l'évaluation");
    m.insert(("mostro.pay_bond", Locale::Fr), "Payer la caution");
    m.insert(("mostro.claim_bond", Locale::Fr), "Réclamer le paiement de la caution");
    m.insert(("mostro.take_with_mostro", Locale::Fr), "Prendre avec Mostro →");
    m.insert(("mostro.taking", Locale::Fr), "En cours…");
    m.insert(("mostro.specify_amount", Locale::Fr), "Spécifier le montant");
    m.insert(("mostro.take_order", Locale::Fr), "Prendre l'ordre");
    m.insert(("mostro.payout_invoice_optional", Locale::Fr), "Facture de paiement (facultatif)");
    m.insert(("mostro.skip", Locale::Fr), "Passer");
    m.insert(("mostro.confirm_release_title", Locale::Fr), "Libérer les sats");
    m.insert(("mostro.confirm_release_body", Locale::Fr),
        "La libération règle l'escrow Lightning et paie l'acheteur. \
         Cette action est irréversible — ne libérez qu'après avoir confirmé \
         la réception du paiement fiat.");
    m.insert(("mostro.release", Locale::Fr), "Libérer");
    m.insert(("mostro.dont_release", Locale::Fr), "Ne pas libérer");
    m.insert(("mostro.confirm_cancel_title", Locale::Fr), "Annuler l'échange");
    m.insert(("mostro.confirm_cancel_body", Locale::Fr),
        "Êtes-vous sûr de vouloir annuler cet échange ? La contrepartie sera notifiée.");
    m.insert(("mostro.yes_cancel", Locale::Fr), "Oui, annuler");
    m.insert(("mostro.keep_trade", Locale::Fr), "Garder l'échange");
    m.insert(("mostro.switch_daemon", Locale::Fr), "Changer de daemon Mostro ?");
    m.insert(("mostro.switch_and_take", Locale::Fr), "Changer et prendre");
    m.insert(("mostro.preferences", Locale::Fr), "Préférences");
    m.insert(("mostro.default_fiat", Locale::Fr), "Monnaie fiat par défaut");
    m.insert(("mostro.default_ln_address", Locale::Fr), "Adresse Lightning par défaut");
    m.insert(("mostro.notifications_label", Locale::Fr), "Notifications");
    m.insert(("mostro.trade_updates", Locale::Fr), "Mises à jour des échanges");
    m.insert(("mostro.chat_messages", Locale::Fr), "Messages de chat");
    m.insert(("mostro.dispute_updates", Locale::Fr), "Mises à jour des litiges");
    m.insert(("mostro.sound", Locale::Fr), "Son");
    m.insert(("mostro.vibration", Locale::Fr), "Vibration");
    m.insert(("mostro.history_expiration", Locale::Fr), "Expiration de l'historique");
    m.insert(("mostro.save_preferences", Locale::Fr), "Sauvegarder les préférences");
    m.insert(("mostro.enable_notifications", Locale::Fr), "Activer les notifications");
    m.insert(("mostro.notifications_enabled", Locale::Fr), "✓ Les notifications sont activées");
    m.insert(("mostro.progress", Locale::Fr), "Progression");
    m.insert(("mostro.in_dispute", Locale::Fr), "En litige");
    m.insert(("mostro.cancel_pending", Locale::Fr), "Annulation en attente");
    m.insert(("mostro.canceled", Locale::Fr), "Annulé");
    m.insert(("mostro.mutual_cancel", Locale::Fr), "Annulation mutuelle");
    m.insert(("mostro.admin_canceled", Locale::Fr), "Annulé par l'admin");
    m.insert(("mostro.expired", Locale::Fr), "Expiré");
    m.insert(("mostro.chat", Locale::Fr), "Chat");
    m.insert(("mostro.chat_locked", Locale::Fr),
        "Le chat sera disponible une fois l'échange actif et la contrepartie révélée.");
    m.insert(("mostro.no_messages", Locale::Fr), "Aucun message pour le moment");
    m.insert(("mostro.download", Locale::Fr), "Télécharger");
    m.insert(("mostro.downloading", Locale::Fr), "Téléchargement…");
    m.insert(("mostro.download_encrypted", Locale::Fr), "Télécharger (chiffré)");
    m.insert(("mostro.no_reputation", Locale::Fr), "Aucune donnée de réputation");
    m.insert(("mostro.counterparty", Locale::Fr), "Contrepartie");
    m.insert(("mostro.rate_received_title", Locale::Fr), "Évaluation reçue");
    m.insert(("mostro.rate_received_body", Locale::Fr),
        "Vous avez reçu une évaluation de {stars} étoiles de la contrepartie.");

    m
}

/// Look up a translated string by key in the active locale.
///
/// Falls back to English, then to the key itself.
pub fn tr(key: &str) -> String {
    let locale = *ACTIVE_LOCALE.read();
    tr_with_locale(key, locale)
}

/// Pure translation function — no Dioxus runtime dependency. Used by `tr()`
/// and by tests.
pub fn tr_with_locale(key: &str, locale: Locale) -> String {
    static STRINGS: std::sync::OnceLock<HashMap<(&'static str, Locale), &'static str>> =
        std::sync::OnceLock::new();
    let map = STRINGS.get_or_init(build_strings);
    map.get(&(key, locale))
        .copied()
        .or_else(|| map.get(&(key, Locale::En)).copied())
        .unwrap_or(key)
        .to_string()
}

/// Detect the browser's locale and set `ACTIVE_LOCALE`.
/// Called at app startup.
pub fn detect_locale() {
    #[cfg(feature = "web")]
    {
        if let Some(window) = web_sys::window() {
            let lang = window.navigator().language().unwrap_or_default();
            let detected = if lang.starts_with("es") {
                Locale::Es
            } else if lang.starts_with("it") {
                Locale::It
            } else if lang.starts_with("de") {
                Locale::De
            } else if lang.starts_with("fr") {
                Locale::Fr
            } else {
                Locale::En
            };
            *ACTIVE_LOCALE.write() = detected;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tr_english() {
        assert_eq!(tr_with_locale("mostro.actions", Locale::En), "Actions");
        assert_eq!(tr_with_locale("mostro.release_sats", Locale::En), "Release Sats");
        assert_eq!(tr_with_locale("mostro.take_with_mostro", Locale::En), "Take with Mostro →");
    }

    #[test]
    fn test_tr_fallback_to_key() {
        assert_eq!(tr_with_locale("mostro.nonexistent_key", Locale::En), "mostro.nonexistent_key");
    }

    #[test]
    fn test_locale_from_str() {
        assert_eq!(Locale::from_str("en"), Locale::En);
        assert_eq!(Locale::from_str("ES"), Locale::Es);
        assert_eq!(Locale::from_str("it"), Locale::It);
        assert_eq!(Locale::from_str("de"), Locale::De);
        assert_eq!(Locale::from_str("fr"), Locale::Fr);
    }

    #[test]
    fn test_locale_matches() {
        assert!(Locale::En.matches("en"));
        assert!(Locale::Es.matches("es"));
        assert!(!Locale::En.matches("es"));
    }

    #[test]
    fn test_string_table_has_entries_for_all_locales() {
        let m = build_strings();
        // Verify at least the "actions" key exists in all 5 locales.
        for locale in [Locale::En, Locale::Es, Locale::It, Locale::De, Locale::Fr] {
            assert!(
                m.contains_key(&("mostro.actions", locale)),
                "missing mostro.actions for {:?}",
                locale
            );
        }
    }
}
