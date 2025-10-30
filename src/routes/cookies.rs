use dioxus::prelude::*;

#[component]
pub fn Cookies() -> Element {
    rsx! {
        div {
            class: "max-w-4xl mx-auto px-6 py-12",
            h1 {
                class: "text-4xl font-bold mb-8",
                "Cookie Policy"
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
                        "1. What Are Cookies?"
                    }
                    p {
                        "Cookies are small text files that are stored on your device when you visit a website. They help websites remember your preferences and provide functionality."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "2. How nostr.blue Uses Cookies"
                    }
                    p {
                        "nostr.blue uses minimal cookies and local storage for essential functionality only. We do NOT use cookies for:"
                    }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { "Advertising or marketing" }
                        li { "User tracking across websites" }
                        li { "Analytics or behavioral profiling" }
                        li { "Third-party data sharing" }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "3. Essential Cookies and Local Storage"
                    }
                    p { "We use browser local storage (not cookies) for:" }

                    div {
                        class: "space-y-4 mt-4",
                        div {
                            class: "border border-border rounded-lg p-4",
                            h3 {
                                class: "font-semibold mb-2",
                                "Authentication"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "Stores your Nostr private keys or session information (if you log in with nsec). This data is encrypted and never leaves your device."
                            }
                            p {
                                class: "text-xs mt-2",
                                strong { "Storage Type: " } "LocalStorage"
                            }
                            p {
                                class: "text-xs",
                                strong { "Duration: " } "Until you log out or clear browser data"
                            }
                        }

                        div {
                            class: "border border-border rounded-lg p-4",
                            h3 {
                                class: "font-semibold mb-2",
                                "User Preferences"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "Remembers your theme preference (light/dark mode), default relays, and other settings."
                            }
                            p {
                                class: "text-xs mt-2",
                                strong { "Storage Type: " } "LocalStorage"
                            }
                            p {
                                class: "text-xs",
                                strong { "Duration: " } "Persistent until cleared"
                            }
                        }

                        div {
                            class: "border border-border rounded-lg p-4",
                            h3 {
                                class: "font-semibold mb-2",
                                "Performance Cache"
                            }
                            p {
                                class: "text-sm text-muted-foreground",
                                "Temporarily caches Nostr events and profile data to improve loading times."
                            }
                            p {
                                class: "text-xs mt-2",
                                strong { "Storage Type: " } "IndexedDB / Cache API"
                            }
                            p {
                                class: "text-xs",
                                strong { "Duration: " } "Varies, automatically cleared when old"
                            }
                        }
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "4. No Third-Party Cookies"
                    }
                    p {
                        "nostr.blue does not use any third-party cookies for analytics, advertising, or tracking. We respect your privacy and do not share your data with advertisers or data brokers."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "5. Managing Cookies and Local Storage"
                    }
                    p {
                        "You can manage or clear cookies and local storage through your browser settings:"
                    }
                    ul {
                        class: "list-disc pl-6 space-y-2",
                        li { strong { "Chrome: " } "Settings → Privacy and security → Cookies and other site data" }
                        li { strong { "Firefox: " } "Settings → Privacy & Security → Cookies and Site Data" }
                        li { strong { "Safari: " } "Preferences → Privacy → Manage Website Data" }
                        li { strong { "Edge: " } "Settings → Cookies and site permissions → Cookies and site data" }
                    }
                    p {
                        class: "mt-4",
                        strong { "Note: " } "Clearing local storage will log you out and reset your preferences."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "6. Browser Extensions"
                    }
                    p {
                        "If you use Nostr browser extensions (like Alby or nos2x) for authentication, those extensions may set their own cookies or storage. Please review their privacy policies."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "7. Changes to Cookie Policy"
                    }
                    p {
                        "We may update this policy to reflect changes in technology or legal requirements. Continued use of nostr.blue constitutes acceptance of any updates."
                    }
                }

                section {
                    class: "space-y-4",
                    h2 {
                        class: "text-2xl font-semibold mt-8",
                        "8. Contact"
                    }
                    p {
                        "For questions about cookies or data storage, visit our "
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
