use dioxus::prelude::*;

#[component]
pub fn About() -> Element {
    rsx! {
        div {
            class: "max-w-4xl mx-auto px-6 py-12",
            h1 {
                class: "text-4xl font-bold mb-8",
                "About nostr.blue"
            }

            div {
                class: "prose dark:prose-invert max-w-none space-y-8",
                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold",
                        "What is nostr.blue?"
                    }
                    p {
                        "nostr.blue is a modern social network client built on the Nostr protocol - a decentralized, censorship-resistant social network. Unlike traditional social media platforms, Nostr gives you true ownership of your identity and content."
                    }
                    p {
                        "With nostr.blue, you can connect with people, share ideas, and participate in communities without relying on centralized corporations that control your data and censor your speech."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold",
                        "Key Features"
                    }
                    div {
                        class: "grid md:grid-cols-2 gap-4 mt-4",
                        div {
                            class: "border border-border rounded-lg p-4",
                            h3 {
                                class: "font-semibold mb-2",
                                "🔐 True Ownership"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "Your identity is controlled by cryptographic keys that only you possess. No company can ban, shadowban, or censor you."
                            }
                        }

                        div {
                            class: "border border-border rounded-lg p-4",
                            h3 {
                                class: "font-semibold mb-2",
                                "⚡ Lightning Zaps"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "Send and receive Bitcoin payments directly on posts using the Lightning Network. Support creators instantly with micropayments."
                            }
                        }

                        div {
                            class: "border border-border rounded-lg p-4",
                            h3 {
                                class: "font-semibold mb-2",
                                "👥 Communities"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "Join NIP-72 moderated communities for topic-specific discussions and collaborations."
                            }
                        }

                        div {
                            class: "border border-border rounded-lg p-4",
                            h3 {
                                class: "font-semibold mb-2",
                                "🔒 Private Messaging"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "End-to-end encrypted direct messages that only you and your recipient can read."
                            }
                        }

                        div {
                            class: "border border-border rounded-lg p-4",
                            h3 {
                                class: "font-semibold mb-2",
                                "📋 Lists & Bookmarks"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "Organize your Nostr experience with custom lists, bookmarks, and curated feeds."
                            }
                        }

                        div {
                            class: "border border-border rounded-lg p-4",
                            h3 {
                                class: "font-semibold mb-2",
                                "📸 Media Support"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "View photos and videos with NIP-71 support for rich multimedia experiences."
                            }
                        }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold",
                        "Technology"
                    }
                    p {
                        "nostr.blue (Rust Edition) is built with modern web technologies for optimal performance and user experience:"
                    }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { strong { "Rust: " } "Systems programming language for performance and safety" }
                        li { strong { "Dioxus: " } "Modern reactive framework for building web interfaces" }
                        li { strong { "rust-nostr: " } "Comprehensive Nostr protocol implementation" }
                        li { strong { "WebAssembly: " } "Near-native performance in the browser" }
                        li { strong { "Tailwind CSS: " } "Utility-first CSS for beautiful designs" }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold",
                        "Open Source"
                    }
                    p {
                        "nostr.blue is open source software. You can view, audit, and contribute to the code on GitHub. We believe in transparency and community-driven development."
                    }
                    div {
                        class: "flex gap-4 mt-4",
                        a {
                            href: "https://github.com/patrickulrich/nostr.blue",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors no-underline",
                            "View on GitHub"
                        }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold",
                        "Nostr Protocol"
                    }
                    p {
                        "Nostr (Notes and Other Stuff Transmitted by Relays) is a simple, open protocol that enables global, decentralized, and censorship-resistant social media. Learn more:"
                    }
                    div {
                        class: "flex gap-4 mt-4",
                        a {
                            href: "https://nostr.com",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "inline-flex items-center gap-2 px-4 py-2 border border-border rounded-lg hover:bg-accent transition-colors no-underline",
                            "nostr.com"
                        }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold",
                        "Privacy & Security"
                    }
                    p {
                        "Your privacy is our priority. nostr.blue operates as a client-side application - your private keys never leave your device, and we don't collect or store your personal data."
                    }
                    p {
                        "Read our "
                        Link {
                            to: crate::routes::Route::Privacy {},
                            class: "text-blue-500 hover:underline",
                            "Privacy Policy"
                        }
                        " and "
                        Link {
                            to: crate::routes::Route::Cookies {},
                            class: "text-blue-500 hover:underline",
                            "Cookie Policy"
                        }
                        " to learn more."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold",
                        "Support the Project"
                    }
                    p {
                        "nostr.blue is free and open source. If you'd like to support development, you can:"
                    }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { "Contribute code on GitHub" }
                        li { "Report bugs and suggest features" }
                        li { "Share nostr.blue with others" }
                        li { "Send Lightning zaps to the developers on Nostr" }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold",
                        "Legal"
                    }
                    p {
                        "By using nostr.blue, you agree to our:"
                    }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li {
                            Link {
                                to: crate::routes::Route::Terms {},
                                class: "text-blue-500 hover:underline",
                                "Terms of Service"
                            }
                        }
                        li {
                            Link {
                                to: crate::routes::Route::Privacy {},
                                class: "text-blue-500 hover:underline",
                                "Privacy Policy"
                            }
                        }
                        li {
                            Link {
                                to: crate::routes::Route::Cookies {},
                                class: "text-blue-500 hover:underline",
                                "Cookie Policy"
                            }
                        }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold",
                        "Acknowledgments"
                    }
                    p {
                        "nostr.blue is built on the shoulders of giants. Special thanks to:"
                    }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li {
                            a {
                                href: "https://rust-nostr.org",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "text-blue-500 hover:underline",
                                "rust-nostr"
                            }
                            " - Comprehensive Nostr protocol implementation"
                        }
                        li {
                            a {
                                href: "https://dioxuslabs.com",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "text-blue-500 hover:underline",
                                "Dioxus"
                            }
                            " - Modern reactive web framework"
                        }
                        li { "The Nostr Community - For building the decentralized web" }
                    }
                }

                footer {
                    class: "text-center text-sm text-muted-foreground mt-12 pt-8 border-t border-border",
                    p {
                        "Built with ⚡ on Nostr | "
                        a {
                            href: "https://github.com/patrickulrich/nostr.blue",
                            class: "text-blue-500 hover:underline",
                            "Open Source"
                        }
                    }
                }
            }
        }
    }
}
