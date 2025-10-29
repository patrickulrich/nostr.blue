use dioxus::prelude::*;

#[component]
pub fn Privacy() -> Element {
    rsx! {
        div {
            class: "max-w-4xl mx-auto px-6 py-12",
            h1 {
                class: "text-4xl font-bold mb-8",
                "Privacy Policy"
            }

            div {
                class: "prose dark:prose-invert max-w-none space-y-6",
                p {
                    class: "text-lg text-muted-foreground",
                    "Last updated: October 29, 2025"
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "1. Overview"
                    }
                    p {
                        "nostr.blue is committed to protecting your privacy. As a client for the decentralized Nostr protocol, we have designed our service to minimize data collection and maximize user control over their information."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "2. Data We Don't Collect"
                    }
                    p {
                        "nostr.blue operates as a client-side application. We do NOT collect, store, or have access to:"
                    }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { "Your private keys (nsec) - these remain in your browser's local storage" }
                        li { "Your personal messages or direct messages" }
                        li { "Your browsing history within the application" }
                        li { "Your IP address for tracking purposes" }
                        li { "Personal information beyond what you choose to publish on Nostr" }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "3. Local Data Storage"
                    }
                    p { "The following data is stored locally in your browser:" }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { "Your Nostr private keys (if you choose to log in with nsec)" }
                        li { "Application preferences (theme, default relays, etc.)" }
                        li { "Cached content for performance" }
                    }
                    p {
                        "This data never leaves your device unless you explicitly publish it to the Nostr network."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "4. Public Data on Nostr"
                    }
                    p { "When you publish content through nostr.blue:" }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { "Posts, reactions, and public interactions are broadcast to Nostr relay servers" }
                        li { "This content becomes publicly accessible on the Nostr network" }
                        li { "Your public key (npub) is associated with your content" }
                        li { "Content may be cached and distributed across multiple relays" }
                    }
                    p {
                        "Remember: Anything you post publicly on Nostr is permanent and decentralized."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "5. Third-Party Services"
                    }
                    p { "nostr.blue connects to:" }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { strong { "Nostr Relays: " } "Independent servers that relay Nostr events" }
                        li { strong { "Lightning Network Services: " } "For zap payments (if you use this feature)" }
                        li { strong { "Media Hosting: " } "When you view images/videos from external URLs" }
                    }
                    p {
                        "These services have their own privacy policies. We recommend reviewing them."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "6. Cookies and Tracking"
                    }
                    p {
                        "nostr.blue uses minimal cookies for essential functionality only. See our "
                        Link {
                            to: crate::routes::Route::Cookies {},
                            class: "text-blue-500 hover:underline",
                            "Cookie Policy"
                        }
                        " for details. We do not use analytics, advertising, or tracking cookies."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "7. Security"
                    }
                    p { "We implement industry-standard security practices:" }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { "HTTPS encryption for all connections" }
                        li { "Client-side cryptography for signing events" }
                        li { "Support for hardware signers and browser extensions" }
                        li { "Regular security audits of our code" }
                    }
                    p {
                        "However, you are responsible for keeping your private keys secure."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "8. Your Rights"
                    }
                    p { "You have the right to:" }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { "Delete your local data by clearing browser storage" }
                        li { "Export your private keys and use them with other Nostr clients" }
                        li { "Request deletion from specific relays (though we cannot guarantee removal)" }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "9. Children's Privacy"
                    }
                    p {
                        "nostr.blue is not intended for users under 13 years of age. We do not knowingly collect information from children."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "10. Changes to Privacy Policy"
                    }
                    p {
                        "We may update this policy from time to time. Continued use of the service after changes constitutes acceptance of the updated policy."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "11. Contact"
                    }
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
