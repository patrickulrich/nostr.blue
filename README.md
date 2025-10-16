# nostr.blue

**A Modern Twitter-like Nostr Client**

nostr.blue is a decentralized social network client built on the Nostr protocol. It provides a familiar Twitter-like experience while leveraging the power of decentralized social networking.

## 🌟 Features

### Core Social Features
- **Home Feed**: Following feed and curated Popular feed powered by DVMs
- **Profile Pages**: View user profiles, follow/unfollow, and see post history
- **Notifications**: Real-time notifications for mentions, reactions, and replies
- **Bookmarks**: Save posts for later reading
- **Lists**: Create and manage user lists with public/private options
- **Communities**: Browse and join NIP-72 communities (moderated Reddit-style groups)

### Advanced Features
- **Data Vending Machines (DVMs)**: Browse DVM services and view curated feeds (NIP-90)
- **Lightning Zaps**: Send and receive Bitcoin payments on posts (NIP-57)
- **Real Zap Counts**: Live zap totals displayed on every post
- **Dark Mode**: System, light, or dark theme with settings saved to Nostr (NIP-78)
- **Settings Sync**: User preferences stored on Nostr and synced across devices

### Content & Interaction
- **Rich Text Posts**: Support for markdown, links, images, and media
- **Reactions**: Like and react to posts
- **Reposts**: Share content with your followers
- **Replies**: Threaded conversations
- **Search**: Discover users and content
- **Trending**: See what's popular on the network

## 🚀 Getting Started

### Prerequisites
- Node.js 18+
- npm or yarn

### Installation

```bash
# Clone the repository
git clone https://github.com/patrickulrich/nostr.blue.git
cd nostr.blue

# Install dependencies
npm install

# Start development server
npm run dev
```

### Building for Production

```bash
npm run build
```

## 🛠 Technology Stack

- **React 18**: Modern React with hooks and concurrent rendering
- **TypeScript**: Type-safe development
- **Vite**: Fast build tool and dev server
- **TailwindCSS**: Utility-first CSS framework
- **shadcn/ui**: Beautiful, accessible UI components
- **Nostrify**: Nostr protocol framework
- **TanStack Query**: Data fetching and caching
- **React Router**: Client-side routing

## 📡 Nostr Protocol Support

nostr.blue implements many Nostr Improvement Proposals (NIPs):

- **NIP-01**: Basic protocol flow and event kinds
- **NIP-02**: Contact/follow lists
- **NIP-07**: Browser extension signing
- **NIP-10**: Text note references and threading
- **NIP-19**: Identifier encoding (npub, note, nevent, naddr, nprofile)
- **NIP-25**: Reactions
- **NIP-44**: Encrypted direct messages
- **NIP-51**: Lists (bookmarks, pin lists, follow sets)
- **NIP-57**: Lightning zaps
- **NIP-72**: Moderated communities
- **NIP-78**: Application-specific data (settings storage)
- **NIP-89**: Recommended application handlers
- **NIP-90**: Data Vending Machines

## 🎨 Features in Detail

### Dark Mode
Settings are saved to Nostr using NIP-78, allowing your theme preference to sync across all devices. Choose from light, dark, or system theme that follows your device preference.

### Communities (NIP-72)
Browse and participate in moderated communities. Communities you're a member of appear at the top of the list with "Member" or "Moderator" badges.

### Data Vending Machines
Discover AI-powered services on Nostr that can:
- Curate content feeds
- Process data
- Provide search and discovery
- Generate recommendations

The Popular feed uses the "Currently Popular Notes DVM" to show trending content across the network.

### Lightning Integration
Send and receive Bitcoin payments directly on posts:
- Zap posts to support creators
- Real-time zap counts displayed
- Multiple payment methods (WebLN, NWC, QR codes)
- View zap leaderboards

### User Lists
Create custom lists of users to:
- Organize follows into categories
- Create private lists
- Share public lists with others
- View feeds filtered by list

## 🔐 Authentication

nostr.blue supports multiple authentication methods:
- Browser extensions (NIP-07) like Alby, nos2x, Flamingo
- nsec (private key) import
- Read-only mode (npub)

## 🌐 Deployment

The app can be deployed to any static hosting service:

```bash
npm run build
```

Deploy the `dist` folder to:
- Vercel
- Netlify
- GitHub Pages
- IPFS
- Any static host

## 🤝 Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

### Development Guidelines
- Use TypeScript for type safety
- Follow existing code structure and patterns
- Test thoroughly before submitting PRs
- Keep components focused and reusable

## 📄 License

Open source - built for the decentralized web.

## 🔗 Links

- **Website**: [nostr.blue](https://nostr.blue)
- **GitHub**: [patrickulrich/nostr.blue](https://github.com/patrickulrich/nostr.blue)
- **Nostr Protocol**: [nostr.com](https://nostr.com)

---

Built with [MKStack](https://soapbox.pub/mkstack) - The Complete Framework for Building Nostr Clients
