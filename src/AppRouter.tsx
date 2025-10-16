import { BrowserRouter, Route, Routes } from "react-router-dom";
import { ScrollToTop } from "./components/ScrollToTop";

import Index from "./pages/Index";
import ExplorePage from "./pages/ExplorePage";
import TrendingPage from "./pages/TrendingPage";
import NotificationsPage from "./pages/NotificationsPage";
import BookmarksPage from "./pages/BookmarksPage";
import ListsPage from "./pages/ListsPage";
import UserListFeedPage from "./pages/UserListFeedPage";
import ManageListPage from "./pages/ManageListPage";
import DVMPage from "./pages/DVMPage";
import DVMFeedPage from "./pages/DVMFeedPage";
import CommunitiesPage from "./pages/CommunitiesPage";
import CommunityFeedPage from "./pages/CommunityFeedPage";
import SettingsPage from "./pages/SettingsPage";
import { NIP19Router } from "./components/NIP19Router";
import NotFound from "./pages/NotFound";

export function AppRouter() {
  return (
    <BrowserRouter>
      <ScrollToTop />
      <Routes>
        <Route path="/" element={<Index />} />
        <Route path="/explore" element={<ExplorePage />} />
        <Route path="/trending" element={<TrendingPage />} />
        <Route path="/notifications" element={<NotificationsPage />} />
        <Route path="/bookmarks" element={<BookmarksPage />} />
        <Route path="/lists" element={<ListsPage />} />
        <Route path="/lists/:listId/manage" element={<ManageListPage />} />
        <Route path="/lists/:listId" element={<UserListFeedPage />} />
        <Route path="/dvm" element={<DVMPage />} />
        <Route path="/dvm/:dvmId" element={<DVMFeedPage />} />
        <Route path="/communities" element={<CommunitiesPage />} />
        <Route path="/community/:aTag" element={<CommunityFeedPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        {/* NIP-19 route for npub1, note1, naddr1, nevent1, nprofile1 */}
        <Route path="/:nip19" element={<NIP19Router />} />
        {/* ADD ALL CUSTOM ROUTES ABOVE THE CATCH-ALL "*" ROUTE */}
        <Route path="*" element={<NotFound />} />
      </Routes>
    </BrowserRouter>
  );
}
export default AppRouter;