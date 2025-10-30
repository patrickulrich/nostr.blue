use dioxus::prelude::*;

#[component]
pub fn Terms() -> Element {
    rsx! {
        div {
            class: "max-w-4xl mx-auto px-6 py-12",
            h1 {
                class: "text-4xl font-bold mb-8",
                "Terms of Service"
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
                        "1. Acceptance of Terms"
                    }
                    p {
                        "By accessing and using nostr.blue, you accept and agree to be bound by the terms and provisions of this agreement. nostr.blue is a decentralized social network client built on the Nostr protocol."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "2. Description of Service"
                    }
                    p {
                        "nostr.blue provides a user interface for accessing the Nostr protocol, a decentralized social network. The service allows users to publish, read, and interact with content on the Nostr network."
                    }
                    p {
                        "As a client for a decentralized protocol, nostr.blue does not host, store, or control the content published through the service. All content is distributed across the Nostr network via independent relay servers."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "3. User Responsibilities"
                    }
                    p { "Users are responsible for:" }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { "Maintaining the security of their private keys and account credentials" }
                        li { "All content they publish through the service" }
                        li { "Complying with applicable laws and regulations" }
                        li { "Respecting the intellectual property rights of others" }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "4. Privacy and Data"
                    }
                    p {
                        "nostr.blue operates as a client-side application. Your private keys and sensitive data are stored locally in your browser and are never transmitted to our servers. See our "
                        Link {
                            to: crate::routes::Route::Privacy {},
                            class: "text-blue-500 hover:underline",
                            "Privacy Policy"
                        }
                        " for more details."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "5. Content and Conduct"
                    }
                    p {
                        "While nostr.blue does not control content on the Nostr network, users agree not to use the service to publish:"
                    }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { "Illegal content" }
                        li { "Content that infringes on intellectual property rights" }
                        li { "Malicious software or code" }
                        li { "Spam or unsolicited commercial content" }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "6. Disclaimer of Warranties"
                    }
                    p {
                        "nostr.blue is provided \"as is\" without warranties of any kind, either express or implied. We do not guarantee the availability, accuracy, or reliability of the service."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "7. Limitation of Liability"
                    }
                    p {
                        "nostr.blue and its operators shall not be liable for any indirect, incidental, special, consequential, or punitive damages resulting from your use of the service."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "8. Changes to Terms"
                    }
                    p {
                        "We reserve the right to modify these terms at any time. Continued use of the service following any changes constitutes acceptance of those changes."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "9. Contact"
                    }
                    p {
                        "For questions about these Terms of Service, please visit our "
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
