use dioxus::prelude::*;
#[component]
pub fn Privacy() -> Element {
    rsx! {
        div { class: "max-w-4xl mx-auto px-6 py-12",
            h1 { class: "text-4xl font-bold mb-8", "Privacy Policy" }
            div { class: "prose dark:prose-invert max-w-none space-y-6",
                p { class: "text-lg text-muted-foreground", "Last updated: March 25, 2026" }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "1. Overview" }
                    p {
                        "nostr.blue is a client for the decentralized Nostr protocol. Most app behavior happens on your device, and we aim to minimize central data collection while giving you control over your keys, relays, wallets, and connected services."
                    }
                    p {
                        "nostr.blue is available as a web app, an Android mobile app, and a native desktop app. The privacy details below apply across those platforms unless a section says otherwise."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "2. Data We Do Not Collect Centrally" }
                    p {
                        "nostr.blue does not run advertising or analytics trackers, and we do not centrally store the following just because you use the app:"
                    }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Your private keys as a server-side account database" }
                        li { "Your personal messages or direct messages in a nostr.blue-hosted mailbox" }
                        li { "Advertising profiles or cross-site tracking data" }
                        li { "A centralized copy of your local app state just for normal app usage" }
                    }
                    p {
                        "However, when you connect to relays, media hosts, wallet services, AI providers, podcast services, geocoding services, or other external systems, those services can receive normal network metadata such as your IP address and request details."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "3. Local Data Stored On Your Device" }
                    p { "Depending on the platform and features you use, nostr.blue stores data locally on your device." }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "If you sign in with an nsec, nostr.blue stores an encrypted `ncryptsec` locally after you protect it with a password" }
                        li { "Your public key, login method, relay preferences, theme, sidebar settings, and other app preferences" }
                        li { "Notification state and other optional settings that may also be synced to Nostr using NIP-78 if you enable those features" }
                        li { "Cashu wallet data and related local state; on web this uses browser storage such as IndexedDB, and on native builds wallet data uses a native database" }
                        li { "AI provider settings, API keys, and AI chat history when you use AI features" }
                        li { "Cached content, offline/service-worker assets on web, and other local performance caches" }
                        li { "On native builds, some settings are stored in local app files, and saved NWC wallet URIs may be written to a restricted local file" }
                    }
                    p {
                        "Local storage is under your device and OS account. It is not automatically uploaded to nostr.blue servers, but some features intentionally publish or sync data to Nostr or other services as described below."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "4. Data Published To Nostr" }
                    p { "When you use nostr.blue to publish or sync data to Nostr, that data is sent to relays you use or configure." }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Public posts, profiles, reactions, follows, relay lists, articles, media references, marketplace data, and other public events are public on the Nostr network" }
                        li { "Your public key is associated with events you publish" }
                        li { "Relays and other clients may copy, cache, index, or mirror data you publish" }
                        li { "If you enable NIP-78 sync, some app settings such as notification read state or preferences may be published to your relays" }
                        li { "If you use Cashu wallet features, nostr.blue may publish encrypted wallet events and encrypted proof events to your relays as part of wallet synchronization" }
                    }
                    p {
                        "Direct messages and other encrypted events are not published as public plaintext, but they still travel through relays and depend on the privacy properties of the relays and signer methods you use."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "5. Third-Party And User-Selected Services" }
                    p { "nostr.blue can connect to third-party or user-selected services. Which ones are used depends on the features you choose." }
                    ul { class: "list-disc pl-6 space-y-2",
                        li {
                            strong { "Nostr Relays: " }
                            "independent servers that receive, store, and relay your Nostr events and subscriptions"
                        }
                        li {
                            strong { "Media Hosts: " }
                            "Blossom servers, NIP-96 servers, RSS/media hosts, and other external URLs used for uploads, downloads, previews, and playback"
                        }
                        li {
                            strong { "Lightning And Wallet Services: " }
                            "WebLN wallets, Nostr Wallet Connect wallets, LNURL endpoints, zap receivers, and Cashu mints if you use payments or wallet features"
                        }
                        li {
                            strong { "Signer Apps And Extensions: " }
                            "browser extensions, NIP-46 remote signers, and Android NIP-55 signer apps if you connect them"
                        }
                        li {
                            strong { "Podcast, Search, And Utility Services: " }
                            "Podcast Index via the app's proxy, RSS sources, configurable mempool endpoints, and Photon geocoding on web when those features are used"
                        }
                        li {
                            strong { "AI Providers: " }
                            "PPQ or any custom OpenAI-compatible provider you configure for AI chat"
                        }
                    }
                    p {
                        "These services operate under their own policies. If you use them, they may receive content, requests, or metadata needed to provide the feature."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "6. Permissions And Device Access" }
                    p {
                        "nostr.blue only asks for device capabilities when a feature needs them."
                    }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Web browsers or webviews may prompt for microphone access when you choose to record voice messages" }
                        li { "Web, mobile, and desktop builds may access the clipboard when you choose copy or paste actions" }
                        li { "The Android app uses system file and image pickers for user-selected files rather than broad storage permissions" }
                        li { "The Android app currently declares Internet, foreground media playback service, and wake-lock permissions for networking and native audio playback" }
                        li { "The Android app uses a foreground media notification/channel for native audio playback" }
                        li { "The Android app can communicate with installed Android signer apps that support NIP-55 if you choose that login method" }
                    }
                    p {
                        "Based on the current Android manifest, nostr.blue does not request location, contacts, or camera permissions."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "7. Security" }
                    p { "Security features in nostr.blue include:" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Client-side cryptography for signing events" }
                        li { "Encrypted local storage for password-protected nsec logins using `ncryptsec`" }
                        li { "Support for browser extensions, remote signers, and Android signer apps so your keys can stay outside nostr.blue" }
                        li { "Encryption of supported wallet events before they are published to relays" }
                    }
                    p {
                        "No client can make decentralized or third-party systems risk-free. You remain responsible for your device security, passwords, relay choices, wallet choices, and any external services you connect."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "8. Cookies, Local Cache, And Offline Data" }
                    p {
                        "On the web app, nostr.blue uses minimal cookies/storage for essential functionality and may use a service worker and local caches for offline support and performance."
                    }
                    p {
                        "We do not use advertising cookies or analytics cookies. See our "
                        Link {
                            to: crate::routes::Route::Cookies {},
                            class: "text-blue-500 hover:underline",
                            "Cookie Policy"
                        }
                        " for more detail."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "9. Your Choices And Controls" }
                    p { "You can:" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Clear browser storage, delete local app data, or uninstall the app to remove data stored on your device" }
                        li { "Choose your own relays, Blossom servers, mempool endpoint, AI provider, signer, and wallet integrations" }
                        li { "Disconnect signers or wallets and remove saved local credentials" }
                        li { "Export your keys and use another Nostr client" }
                        li { "Request deletion from specific relays or media hosts, though decentralized or mirrored data may persist elsewhere" }
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "10. Children's Privacy" }
                    p {
                        "nostr.blue is not intended for users under 13 years of age. We do not knowingly collect information from children."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "11. Changes To This Policy" }
                    p {
                        "We may update this policy as nostr.blue changes. The date at the top of this page reflects the latest revision."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "12. Contact" }
                    p {
                        "For privacy questions, please visit our "
                        Link {
                            to: crate::routes::Route::About {},
                            class: "text-blue-500 hover:underline",
                            "About"
                        }
                        " page."
                    }
                }
            }
        }
    }
}
