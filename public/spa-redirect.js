// SPA Redirect Handler for GitHub Pages 404
(function() {
    // Check for control characters using character codes (Biome-compliant)
    function hasControlCharacters(str) {
        for (let i = 0; i < str.length; i++) {
            const code = str.charCodeAt(i);
            if (code <= 0x1F || code === 0x7F) return true;
        }
        return false;
    }

    const redirect = sessionStorage.getItem('spa-redirect');
    if (redirect) {
        sessionStorage.removeItem('spa-redirect');
        // Validate redirect is a safe same-origin path
        if (redirect !== '/' && redirect !== '/index.html' &&
            typeof redirect === 'string' &&
            redirect.length > 0 &&
            redirect.startsWith('/') &&
            !redirect.startsWith('//') &&
            !redirect.startsWith('/\\') &&
            !redirect.includes('://') &&
            !hasControlCharacters(redirect)) {
            history.replaceState(null, '', redirect);
        }
    }
})();
