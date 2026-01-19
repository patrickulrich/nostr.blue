// SPA Redirect Handler for GitHub Pages 404
(function() {
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
            !/[\r\n\x00-\x1f]/.test(redirect)) {
            history.replaceState(null, '', redirect);
        }
    }
})();
