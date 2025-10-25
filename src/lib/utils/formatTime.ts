/**
 * Format a timestamp as a relative time string (e.g., "2 hours ago")
 * @param timestamp - Unix timestamp in seconds
 * @returns Formatted relative time string
 */
export function formatDistanceToNow(timestamp: number): string {
	const now = Date.now();
	const diffMs = now - timestamp * 1000;
	const diffSecs = Math.floor(diffMs / 1000);
	const diffMins = Math.floor(diffMs / 60000);
	const diffHours = Math.floor(diffMs / 3600000);
	const diffDays = Math.floor(diffMs / 86400000);

	if (diffSecs < 60) return 'just now';
	if (diffMins < 60) return `${diffMins}m ago`;
	if (diffHours < 24) return `${diffHours}h ago`;
	if (diffDays < 7) return `${diffDays}d ago`;

	// For older posts, show the date
	const date = new Date(timestamp * 1000);
	return date.toLocaleDateString('en-US', {
		month: 'short',
		day: 'numeric',
		...(date.getFullYear() !== new Date().getFullYear() ? { year: 'numeric' } : {})
	});
}
