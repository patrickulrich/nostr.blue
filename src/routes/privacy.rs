use dioxus::prelude::*;
#[component]
pub fn Privacy() -> Element {
    rsx! {
        div { class: "max-w-4xl mx-auto px-6 py-12",
            h1 { class: "text-4xl font-bold mb-8", "Privacy Policy" }
            div { class: "prose dark:prose-invert max-w-none space-y-6",
                p { class: "text-lg text-muted-foreground", "Last updated: May 18, 2026" }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "1. Overview" }
                    p {
                        "nostr.blue is a client for the decentralized Nostr protocol. Most app behavior happens on your device, and we aim to minimize central data collection while giving you control over your keys, relays, wallets, and connected services."
                    }
                    p {
                        "nostr.blue is available as a web app hosted on GitHub Pages, an Android app distributed via the Zapstore and Google Play Store, and a native desktop app. The privacy details below apply across those platforms unless a section says otherwise."
                    }
                    p {
                        "The web app is served through GitHub Pages, which means GitHub (Microsoft) may log standard web request metadata such as IP addresses as part of their hosting infrastructure. nostr.blue does not control GitHub's logging practices."
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
                        li { "Google account passwords, email addresses, profile names, profile pictures, or persistent Google access tokens" }
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
                        li { "Cloud backup state (Google user ID and access token) is held only in device memory during active backup or restore operations and is not persisted to disk" }
                    }
                    p {
                        "Local storage is under your device and OS account. It is not automatically uploaded to nostr.blue servers, but some features intentionally publish or sync data to Nostr or other services as described below."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "4. Google User Data — Access, Use, and Disclosure" }
                    p {
                        "nostr.blue integrates with Google services solely for the optional encrypted cloud backup feature (described in detail in section 5). This section specifically addresses how nostr.blue interacts with Google user data in compliance with the Google API Services User Data Policy and Google APIs Terms of Service."
                    }
                    h3 { class: "text-xl font-semibold mt-6", "4a. Google User Data We Access" }
                    p { "When you use the optional cloud backup feature, nostr.blue accesses the following Google user data:" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li {
                            strong { "Google User ID (`sub` claim): " }
                            "A unique, stable identifier for your Google account, obtained from the OpenID Connect ID token. On the web app, this is retrieved via the Google OAuth2 token info endpoint. On Android, this is decoded from the JWT ID token returned by Android CredentialManager."
                        }
                        li {
                            strong { "Short-lived OAuth 2.0 access token: " }
                            "A temporary bearer token used to authenticate Drive API requests. This token expires after a short period and is not a refresh token."
                        }
                        li {
                            strong { "Google Drive `appDataFolder`: " }
                            "A hidden, app-only storage space within your Google Drive that is not visible in your normal Drive files. nostr.blue reads and writes a single encrypted file in this folder."
                        }
                    }
                    p {
                        strong { "What we do NOT access: " }
                        "nostr.blue does not access your Google email address, profile name, profile picture, contacts, calendar, general Google Drive files, or any other Google account data. On Android, the Google ID token JWT technically contains email and name fields, but nostr.blue only reads the `sub` (user ID) field and ignores all other claims. No email or profile scopes are requested."
                    }
                    h3 { class: "text-xl font-semibold mt-6", "4b. How We Use Google User Data" }
                    p { "Each piece of Google user data is used for a specific, limited purpose:" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li {
                            strong { "Google User ID (`sub`): " }
                            "Used as input to an HMAC-SHA256 key derivation function (with the static salt \"nostrblue-backup-v1\") to derive a NIP-44 encryption key. This key is used to encrypt your backup before it leaves your device. The user ID is never sent to Google or any server as part of this process — it is only used locally in your browser or app."
                        }
                        li {
                            strong { "OAuth access token: " }
                            "Used as a Bearer token in the Authorization header of Google Drive REST API v3 calls to list, create, read, and delete files within your `appDataFolder`. The token is used only during active backup or restore operations."
                        }
                        li {
                            strong { "`appDataFolder`: " }
                            "Used to store a single encrypted backup file containing your encrypted Nostr private key and optional wallet connection URI."
                        }
                    }
                    p {
                        "Google user data is used solely to provide the optional encrypted cloud backup feature. It is not used for advertising, analytics, marketing, profiling, content personalization, or any other purpose."
                    }
                    h3 { class: "text-xl font-semibold mt-6", "4c. How We Store Google User Data" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li {
                            strong { "In memory (transient): " }
                            "The Google User ID (`sub`) and OAuth access token are held only in volatile device memory (RAM) during active backup or restore operations. They are never written to disk, never stored in databases, and never sent to nostr.blue servers. They are cleared when the operation completes or the app is closed."
                        }
                        li {
                            strong { "On device (persistent): " }
                            "Only a single boolean flag (`google_backup_user`) is persisted to local device storage to indicate that the cloud backup feature has been used. No Google tokens, user IDs, or other Google data are persisted to disk."
                        }
                        li {
                            strong { "On Google Drive: " }
                            "A single encrypted file (`nostrblue_backup_{{npub}}.bin`) is stored in your Google Drive `appDataFolder`. The file contents are encrypted with NIP-44 (XChaCha20-Poly1305) using a client-side derived key. The file persists until you delete it from within nostr.blue or revoke access at myaccount.google.com/permissions."
                        }
                    }
                    p {
                        "No Google access tokens, refresh tokens, or user IDs are ever persisted to disk or stored in any database."
                    }
                    h3 { class: "text-xl font-semibold mt-6", "4d. How We Share Google User Data" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Google user data is not shared with any third parties." }
                        li { "It is not used for advertising, analytics, or marketing purposes." }
                        li { "It is not transferred, sold, or disclosed to any other party except as required to perform the Google Drive backup operations described above." }
                        li {
                            "Google itself receives standard OAuth metadata (that nostr.blue requested access) and the encrypted backup file stored in your `appDataFolder`. Google cannot read the backup contents because the encryption key is derived client-side and never sent to Google."
                        }
                    }
                    h3 { class: "text-xl font-semibold mt-6", "4e. Google API Scopes Used" }
                    p { "nostr.blue requests the following OAuth scopes:" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li {
                            code { "https://www.googleapis.com/auth/drive.appdata" }
                            " — read and write access to the app's hidden `appDataFolder` in Google Drive. This scope does not grant access to your general Google Drive files."
                        }
                        li {
                            code { "openid" }
                            " — OpenID Connect scope used to obtain your Google user ID (`sub` claim). No email or profile scopes are requested."
                        }
                    }
                    p {
                        "Both scopes are requested together only when you choose to use the cloud backup feature. They are not requested at app launch or for any other purpose."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "5. Google Drive Cloud Backup" }
                    p {
                        "nostr.blue offers an optional encrypted private key backup feature using Google Drive. This feature is not enabled by default and only activates when you choose to use it. See section 4 for details on how Google user data is accessed, used, stored, and shared."
                    }
                    h3 { class: "text-xl font-semibold mt-4", "What You Share With Google" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li {
                            strong { "Google Sign-In: " }
                            "On the web app, nostr.blue uses Google Identity Services. On Android, it uses Android CredentialManager. Both ask you to sign in to your Google account and grant access to the `drive.appdata` and `openid` scopes."
                        }
                        li {
                            strong { "What Google Receives: " }
                            "Google receives standard OAuth sign-in metadata. Google knows that nostr.blue requested access to your Drive appDataFolder, but does not receive your Nostr keys or any Nostr-related data."
                        }
                        li {
                            strong { "What nostr.blue Receives: " }
                            "Your Google user ID (`sub`) and a short-lived access token. The `openid` scope is used solely to obtain your Google user ID (`sub`). Your Google email address, name, and profile picture are never read, stored, or processed — even though they may be present in the ID token on Android. These values are held only in device memory during the backup or restore session and are not stored persistently."
                        }
                    }
                    h3 { class: "text-xl font-semibold mt-4", "What Is Stored In Google Drive" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li {
                            "A single file is stored in your Google Drive "
                            code { "appDataFolder" }
                            " — a hidden, app-only storage area that is not visible in your normal Google Drive files."
                        }
                        li {
                            "The file contains your encrypted private key (nsec) and optionally your NWC wallet URI, encrypted with NIP-44 encryption using a key derived from your Google user ID via HMAC-SHA256."
                        }
                        li {
                            "nostr.blue cannot decrypt your backup without access to your Google credentials. Google cannot read the backup contents because the encryption key is derived client-side and never sent to Google."
                        }
                    }
                    h3 { class: "text-xl font-semibold mt-4", "Managing Your Backup" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "You can create, re-create, or delete your cloud backup at any time from nostr.blue settings" }
                        li { "You can revoke nostr.blue's access to your Google Drive at myaccount.google.com/permissions" }
                        li { "Deleting your nostr.blue account data or uninstalling the app does not automatically delete the Google Drive backup file — you must delete it from within nostr.blue or revoke access at Google" }
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "6. Data Published To Nostr" }
                    p { "When you use nostr.blue to publish or sync data to Nostr, that data is sent to relays you use or configure." }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Public posts, profiles, reactions, follows, relay lists, articles, media references, marketplace data, and other public events are public on the Nostr network" }
                        li { "Your public key is associated with events you publish" }
                        li { "Relays and other clients may copy, cache, index, or mirror data you publish" }
                        li { "If you enable NIP-78 sync, some app settings such as notification read state or preferences may be published to your relays" }
                        li { "If you use Cashu wallet features, nostr.blue publishes encrypted wallet state events (NIP-60 kinds 7374, 7375, 7376) and deletion events to your relays — all encrypted with NIP-44 before publishing" }
                    }
                    p {
                        "Direct messages and other encrypted events are not published as public plaintext, but they still travel through relays and depend on the privacy properties of the relays and signer methods you use."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "7. Third-Party And User-Selected Services" }
                    p { "nostr.blue can connect to third-party or user-selected services. Which ones are used depends on the features you choose." }
                    ul { class: "list-disc pl-6 space-y-2",
                        li {
                            strong { "Nostr Relays: " }
                            "independent servers that receive, store, and relay your Nostr events and subscriptions. Relay operators can see your IP address and the content of events you publish or subscribe to."
                        }
                        li {
                            strong { "Media Hosts: " }
                            "Blossom servers, NIP-96 servers (such as nostr.build), RSS/media hosts, and other external URLs used for uploads, downloads, previews, and playback. Uploads to Blossom servers are authenticated with NIP-98 HTTP Auth. Media hosts receive your files and standard request metadata."
                        }
                        li {
                            strong { "Lightning And Wallet Services: " }
                            "WebLN wallets, Nostr Wallet Connect (NWC) wallets, LNURL endpoints, zap receivers, and Cashu mints if you use payments or wallet features. NWC wallet URIs you save are stored locally with restricted permissions. When you use zap features, your signer creates zap request events that are sent to relays and LNURL endpoints."
                        }
                        li {
                            strong { "Signer Apps And Extensions: " }
                            "browser extensions, NIP-46 remote signers (bunker), and Android NIP-55 signer apps if you connect them. Remote signers receive the content of events they are asked to sign. Android signer apps receive unsigned event data via the Android inter-app communication system."
                        }
                        li {
                            strong { "Podcast And Music Services: " }
                            "Podcast Index API accessed via the app's proxy server. Requests are authenticated with NIP-98 HTTP Auth, meaning your Nostr public key and a signed event are sent to the proxy. The proxy forwards requests to Podcast Index. RSS feed URLs for music and podcasts are also fetched through this proxy for chapter data and transcripts."
                        }
                        li {
                            strong { "AI Providers: " }
                            "PPQ (api.ppq.ai) or any custom OpenAI-compatible or Anthropic-compatible provider you configure. Your chat messages, conversation history, and any data fetched by AI tools (such as Nostr events from your relays) are sent to the AI provider you select. PPQ account credentials (a created account key, an imported Credit ID, or a managed API key) are stored locally and synced only through your encrypted Nostr preferences. If you enable NWC auto-topup with PPQ, your NWC wallet connection URI is shared with PPQ's servers."
                        }
                        li {
                            strong { "Geocoding Services (Web Only): " }
                            "Location search queries are sent to Photon (komoot.io) and Nominatim (openstreetmap.org) when you use location features. Results are cached locally. These services receive your search terms and IP address."
                        }
                        li {
                            strong { "Wavlake: " }
                            "Public API at wavlake.com used for music search and playback. Receives search queries and track/album/artist IDs. No authentication is required."
                        }
                    }
                    p {
                        "These services operate under their own privacy policies. If you use them, they may receive content, requests, or metadata needed to provide the feature."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "8. Permissions And Device Access" }
                    p {
                        "nostr.blue only asks for device capabilities when a feature needs them."
                    }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Web browsers or webviews may prompt for microphone access when you choose to record voice messages" }
                        li { "Web, mobile, and desktop builds may access the clipboard when you choose copy or paste actions" }
                        li { "The Android app uses system file and image pickers for user-selected files rather than broad storage permissions" }
                        li { "The Android app currently declares Internet, foreground media playback service, wake-lock, and notification permissions for networking and native audio playback" }
                        li { "The Android app uses a foreground media notification/channel for native audio playback" }
                        li { "The Android app can communicate with installed Android signer apps that support NIP-55 if you choose that login method" }
                        li { "The Android app uses Google Play Services (CredentialManager and AuthorizationClient) for Google Sign-In when you use the cloud backup feature" }
                    }
                    p {
                        "Based on the current Android manifest, nostr.blue does not request location, contacts, camera, or microphone permissions on Android."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "9. Security" }
                    p { "Security features in nostr.blue include:" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Client-side cryptography for signing events" }
                        li { "Encrypted local storage for password-protected nsec logins using `ncryptsec`" }
                        li { "Support for browser extensions, remote signers, and Android signer apps so your keys can stay outside nostr.blue" }
                        li { "Encryption of wallet events (NIP-44) and other sensitive events before they are published to relays" }
                        li { "NIP-44 end-to-end encryption for Google Drive cloud backups — the encryption key is derived client-side from your Google user ID and never sent to Google or any server" }
                        li { "Secure memory handling for sensitive data using zeroize patterns" }
                        li { "NIP-98 HTTP Auth for authenticating with third-party services without sharing your private key" }
                    }
                    p {
                        "No client can make decentralized or third-party systems risk-free. You remain responsible for your device security, passwords, relay choices, wallet choices, and any external services you connect."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "10. Cookies, Local Cache, And Offline Data" }
                    p {
                        "On the web app, nostr.blue uses minimal cookies/storage for essential functionality and may use a service worker and local caches for offline support and performance."
                    }
                    p {
                        "The service worker caches static assets (HTML, CSS, JavaScript, WebAssembly) for offline access and checks for app updates periodically. No tracking or analytics data is collected through the service worker."
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
                    h2 { class: "text-2xl font-semibold mt-8", "11. Distribution Platforms" }
                    p { "nostr.blue is distributed through several channels, each of which may collect their own metadata:" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li {
                            strong { "GitHub Pages (Web): " }
                            "The web app is hosted on GitHub Pages. GitHub may log standard HTTP request metadata (IP addresses, request times, user agents) as part of their hosting infrastructure. See GitHub's privacy policy for details."
                        }
                        li {
                            strong { "Google Play Store (Android): " }
                            "If you installed nostr.blue from the Google Play Store, Google collects standard install metadata (device model, OS version, install/uninstall events). nostr.blue does not send any app-internal data to Google Play Services beyond what is required for Google Sign-In when you use the cloud backup feature."
                        }
                        li {
                            strong { "Zapstore (Android): " }
                            "If you installed nostr.blue via Zapstore, the Zapstore app handles its own update checking and distribution. nostr.blue does not send data to Zapstore."
                        }
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "12. Your Choices And Controls" }
                    p { "You can:" }
                    ul { class: "list-disc pl-6 space-y-2",
                        li { "Clear browser storage, delete local app data, or uninstall the app to remove data stored on your device" }
                        li { "Choose your own relays, Blossom servers, mempool endpoint, AI provider, signer, and wallet integrations" }
                        li { "Disconnect signers or wallets and remove saved local credentials" }
                        li { "Export your keys and use another Nostr client" }
                        li { "Delete your Google Drive cloud backup from within nostr.blue settings, or revoke nostr.blue's Google Drive access at myaccount.google.com/permissions" }
                        li { "Request deletion from specific relays or media hosts, though decentralized or mirrored data may persist elsewhere" }
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "13. Children's Privacy" }
                    p {
                        "nostr.blue is not intended for users under 13 years of age. We do not knowingly collect information from children."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "14. Changes To This Policy" }
                    p {
                        "We may update this policy as nostr.blue changes. The date at the top of this page reflects the latest revision."
                    }
                }
                section { class: "space-y-4",
                    h2 { class: "text-2xl font-semibold mt-8", "15. Contact" }
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
