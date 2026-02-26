// Git Web Worker using isomorphic-git
// Handles git operations off the main thread for nostr.blue

import git from 'isomorphic-git';
import http from 'isomorphic-git/http/web';
import LightningFS from '@isomorphic-git/lightning-fs';
import { Buffer } from 'buffer';

// Required for isomorphic-git
globalThis.Buffer = Buffer;

function sanitizeFilepath(fp) {
    if (typeof fp !== 'string') fp = String(fp);
    const cleaned = fp.split('').filter(c => {
        const code = c.charCodeAt(0);
        return code > 0x1F && code !== 0x7F;
    }).join('');
    const segments = cleaned.split('/');
    if (segments.some(s => s === '..' || s === '.')) {
        throw new Error('Path traversal detected: ' + fp);
    }
    return cleaned;
}

// Initialize virtual filesystem backed by IndexedDB
const fs = new LightningFS('nostr-blue-git-cache');

// CORS proxy for GitHub/GitLab/etc
const CORS_PROXY = 'https://cors.isomorphic-git.org';

/**
 * Resolve a git ref with fallback to default branches
 * First tries the requested ref, then falls back to main/master local branches,
 * then tries remote branches if no local branches exist.
 */
/**
 * Check if a blob appears to be binary by scanning for null bytes
 * in the first 8 KB of data.
 * @param {Uint8Array} uint8Array - The blob content to check
 * @returns {boolean} true if the content appears to be binary
 */
function isBinary(uint8Array) {
  // Detect UTF-32 BOM before UTF-16 (UTF-32 LE starts with 0xFF 0xFE like UTF-16 LE)
  if (uint8Array.length >= 4) {
    if ((uint8Array[0] === 0xFF && uint8Array[1] === 0xFE && uint8Array[2] === 0x00 && uint8Array[3] === 0x00) ||
        (uint8Array[0] === 0x00 && uint8Array[1] === 0x00 && uint8Array[2] === 0xFE && uint8Array[3] === 0xFF)) {
      return false; // UTF-32 text, not binary
    }
  }

  // Detect UTF-16 BOM — these files are text, not binary,
  // but their encoding contains null bytes that would trip the scan below.
  if (uint8Array.length >= 2) {
    if ((uint8Array[0] === 0xFF && uint8Array[1] === 0xFE) ||
        (uint8Array[0] === 0xFE && uint8Array[1] === 0xFF)) {
      return false; // UTF-16 text, not binary
    }
  }

  const scanLen = Math.min(uint8Array.length, 8192);
  for (let i = 0; i < scanLen; i++) {
    if (uint8Array[i] === 0) return true;
  }
  return false;
}

async function resolveRefWithFallback(dir, ref) {
  // Try direct resolution first
  try {
    return await git.resolveRef({ fs, dir, ref });
  } catch (e) {
    // If HEAD resolution fails, try to find any available branch
    const branches = await git.listBranches({ fs, dir });
    if (branches.length > 0) {
      // Try common default branch names first
      const defaultBranch = branches.find(b => b === 'main')
        || branches.find(b => b === 'master')
        || branches[0];
      return await git.resolveRef({ fs, dir, ref: defaultBranch });
    }

    // No local branches, try remote branches
    const remoteBranches = await git.listBranches({ fs, dir, remote: 'origin' });
    if (remoteBranches.length > 0) {
      const defaultBranch = remoteBranches.find(b => b === 'main')
        || remoteBranches.find(b => b === 'master')
        || remoteBranches[0];
      return await git.resolveRef({ fs, dir, ref: `refs/remotes/origin/${defaultBranch}` });
    }

    throw new Error('No branches found in repository');
  }
}

// Domains that need CORS proxy
const NEEDS_PROXY = ['github.com', 'gitlab.com', 'codeberg.org', 'gitea.com'];

// Known GRASP servers (have CORS enabled per spec)
// Initialized with fallback servers, dynamically updated via updateGraspServers message
let graspServers = new Set([
  'relay.ngit.dev',
  'gitnostr.com',
  'ngit.danconwaydev.com',
  'git.shakespeare.diy',
  'git-01.uid.ovh',
  'git-02.uid.ovh',
  'git.jb55.com',
]);

/**
 * Check if a URL needs CORS proxy
 */
function needsProxy(url) {
  try {
    const parsed = new URL(url);
    // GRASP servers don't need proxy
    if (graspServers.has(parsed.hostname)) return false;
    // Check if hostname matches or is subdomain of blocked domains
    // This prevents matching malicious subdomains like github.com.malicious.com
    return NEEDS_PROXY.some(domain =>
      parsed.hostname === domain || parsed.hostname.endsWith('.' + domain)
    );
  } catch {
    // Invalid URL, use proxy as fallback
    return true;
  }
}

/**
 * Get CORS proxy URL if needed
 */
function getCorsProxy(url) {
  return needsProxy(url) ? CORS_PROXY : undefined;
}

/**
 * Validate directory path to prevent path traversal attacks.
 * Checks for null bytes, '..' sequences, and absolute paths.
 * @param {string} dir - Directory path to validate
 * @throws {Error} If path is invalid
 */
function validateRepoDir(dir) {
  // Type guard: ensure dir is a string before calling string methods
  if (typeof dir !== 'string') {
    throw new Error('Invalid directory path');
  }
  if (!dir || dir.includes('..') || dir.includes('\0')) {
    throw new Error('Invalid directory path');
  }
  // Normalize but preserve leading / for LightningFS absolute paths
  const parts = dir.split('/').filter(p => p && p !== '.');
  const normalized = (dir.startsWith('/') ? '/' : '') + parts.join('/');
  if (normalized === '' || normalized === '/') {
    throw new Error('Invalid repository directory path');
  }
  return normalized;
}

/**
 * Normalize line endings and split content into lines.
 * Replaces CRLF with LF, splits on LF, removes trailing empty element.
 */
function normalizeAndSplit(content) {
  const lines = content.replace(/\r\n?/g, '\n').split('\n');
  if (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
  return lines;
}

/**
 * Compute an edit script between two line arrays using an LCS-based algorithm.
 * Returns an array of operations: { type: 'equal'|'delete'|'insert', oldIdx, newIdx }
 */
function computeEdits(oldLines, newLines) {
  const N = oldLines.length;
  const M = newLines.length;

  // Both arrays empty — nothing to diff
  if (N === 0 && M === 0) return [];

  // For very large files, fall back to simple line-by-line to avoid memory issues
  if (N + M > 20000) {
    return computeEditsFallback(oldLines, newLines);
  }

  // Myers diff algorithm (forward only, linear space for trace)
  const max = N + M;
  const vSize = 2 * max + 1;
  const v = new Int32Array(vSize);
  v.fill(-1);
  const offset = max;
  v[offset + 1] = 0;

  // Store trace for backtracking
  const trace = [];

  // OOM guard: track cumulative memory from trace snapshots
  const SAFE_TRACE_BYTES = 64 * 1024 * 1024;
  let cumulativeBytes = 0;

  outer:
  for (let d = 0; d <= max; d++) {
    // Check memory before storing trace snapshot
    cumulativeBytes += vSize * 4;
    if (cumulativeBytes > SAFE_TRACE_BYTES) {
      return computeEditsFallback(oldLines, newLines);
    }
    // Save current v state for backtracking
    trace.push(v.slice());

    for (let k = -d; k <= d; k += 2) {
      let x;
      if (k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1])) {
        x = v[offset + k + 1]; // move down (insert)
      } else {
        x = v[offset + k - 1] + 1; // move right (delete)
      }
      let y = x - k;

      // Follow diagonal (equal lines)
      while (x < N && y < M && oldLines[x] === newLines[y]) {
        x++;
        y++;
      }

      v[offset + k] = x;

      if (x >= N && y >= M) {
        break outer;
      }
    }
  }

  // Backtrack to build edit script
  const edits = [];
  let x = N;
  let y = M;

  for (let d = trace.length - 1; d > 0; d--) {
    const prev = trace[d];
    const k = x - y;

    let prevK;
    if (k === -d || (k !== d && prev[offset + k - 1] < prev[offset + k + 1])) {
      prevK = k + 1; // came from above (insert)
    } else {
      prevK = k - 1; // came from left (delete)
    }

    const prevX = prev[offset + prevK];
    const prevY = prevX - prevK;

    // Diagonal (equal)
    while (x > prevX && y > prevY) {
      x--;
      y--;
      edits.push({ type: 'equal', oldIdx: x, newIdx: y });
    }

    if (x === prevX) {
      // Insert
      y--;
      edits.push({ type: 'insert', newIdx: y });
    } else {
      // Delete
      x--;
      edits.push({ type: 'delete', oldIdx: x });
    }
  }

  // Handle remaining diagonal at d=0
  while (x > 0 && y > 0) {
    x--;
    y--;
    edits.push({ type: 'equal', oldIdx: x, newIdx: y });
  }

  // Handle remaining deletes/inserts at the beginning
  while (x > 0) { x--; edits.push({ type: 'delete', oldIdx: x }); }
  while (y > 0) { y--; edits.push({ type: 'insert', newIdx: y }); }

  edits.reverse();
  return edits;
}

/**
 * Fallback diff for very large files: simple LCS with hash bucketing
 */
function computeEditsFallback(oldLines, newLines) {
  const edits = [];
  let oi = 0, ni = 0;

  // Build a map of new lines for quick lookup
  const newMap = new Map();
  for (let i = 0; i < newLines.length; i++) {
    if (!newMap.has(newLines[i])) newMap.set(newLines[i], []);
    newMap.get(newLines[i]).push(i);
  }

  while (oi < oldLines.length && ni < newLines.length) {
    if (oldLines[oi] === newLines[ni]) {
      edits.push({ type: 'equal', oldIdx: oi, newIdx: ni });
      oi++;
      ni++;
    } else {
      // Look ahead in both to find next match
      let foundOld = -1, foundNew = -1;
      const lookAhead = Math.min(500, Math.max(oldLines.length - oi, newLines.length - ni));

      for (let look = 1; look < lookAhead; look++) {
        if (oi + look < oldLines.length && oldLines[oi + look] === newLines[ni]) {
          foundOld = oi + look;
          break;
        }
      }

      // Use newMap for O(1) lookup of oldLines[oi] in newLines
      const candidates = newMap.get(oldLines[oi]);
      if (candidates) {
        for (const idx of candidates) {
          if (idx > ni) {
            foundNew = idx;
            break;
          }
        }
      }

      if (foundOld >= 0 && (foundNew < 0 || foundOld - oi <= foundNew - ni)) {
        // Delete lines until match
        while (oi < foundOld) {
          edits.push({ type: 'delete', oldIdx: oi });
          oi++;
        }
      } else if (foundNew >= 0) {
        // Insert lines until match
        while (ni < foundNew) {
          edits.push({ type: 'insert', newIdx: ni });
          ni++;
        }
      } else {
        // No nearby match, emit paired delete+insert and advance both indices
        edits.push({ type: 'delete', oldIdx: oi });
        edits.push({ type: 'insert', newIdx: ni });
        oi++;
        ni++;
      }
    }
  }

  // Remaining old lines are deletes
  while (oi < oldLines.length) {
    edits.push({ type: 'delete', oldIdx: oi });
    oi++;
  }
  // Remaining new lines are inserts
  while (ni < newLines.length) {
    edits.push({ type: 'insert', newIdx: ni });
    ni++;
  }

  return edits;
}

/**
 * Generate unified diff hunks from an edit script.
 * @param {Array} edits - Edit operations from computeEdits
 * @param {Array} oldLines - Original file lines
 * @param {Array} newLines - Modified file lines
 * @param {number} contextLines - Number of context lines around changes (default 3)
 * @returns {Array} Array of diff output lines (hunk headers + content)
 */
function generateHunks(edits, oldLines, newLines, contextLines = 3) {
  if (edits.length === 0) return [];

  const output = [];

  // Find change regions (non-equal edits)
  const changes = [];
  for (let i = 0; i < edits.length; i++) {
    if (edits[i].type !== 'equal') {
      changes.push(i);
    }
  }

  if (changes.length === 0) return [];

  // Group changes into hunks (merge when context lines overlap)
  const hunkGroups = [];
  let currentGroup = [changes[0]];

  for (let i = 1; i < changes.length; i++) {
    // Count equal lines between previous change and this one
    let equalCount = 0;
    for (let j = changes[i - 1] + 1; j < changes[i]; j++) {
      if (edits[j].type === 'equal') equalCount++;
    }
    if (equalCount < contextLines * 2) {
      // Merge into current hunk
      currentGroup.push(changes[i]);
    } else {
      hunkGroups.push(currentGroup);
      currentGroup = [changes[i]];
    }
  }
  hunkGroups.push(currentGroup);

  // Generate output for each hunk group
  for (const group of hunkGroups) {
    const firstChange = group[0];
    const lastChange = group[group.length - 1];

    // Find context boundaries
    let startEdit = firstChange;
    let contextBefore = 0;
    for (let i = firstChange - 1; i >= 0 && contextBefore < contextLines; i--) {
      if (edits[i].type === 'equal') {
        startEdit = i;
        contextBefore++;
      }
    }

    let endEdit = lastChange;
    let contextAfter = 0;
    for (let i = lastChange + 1; i < edits.length && contextAfter < contextLines; i++) {
      if (edits[i].type === 'equal') {
        endEdit = i;
        contextAfter++;
      }
    }

    // Compute hunk line numbers
    let oldStart = -1, oldCount = 0;
    let newStart = -1, newCount = 0;
    const hunkLines = [];

    for (let i = startEdit; i <= endEdit; i++) {
      const edit = edits[i];
      if (edit.type === 'equal') {
        if (oldStart === -1) oldStart = edit.oldIdx;
        if (newStart === -1) newStart = edit.newIdx;
        oldCount++;
        newCount++;
        hunkLines.push(` ${oldLines[edit.oldIdx]}`);
      } else if (edit.type === 'delete') {
        if (oldStart === -1) oldStart = edit.oldIdx;
        if (newStart === -1) {
          // Find next equal or insert to determine newStart
          for (let j = i + 1; j <= endEdit; j++) {
            if (edits[j].type === 'equal') { newStart = edits[j].newIdx; break; }
            if (edits[j].type === 'insert') { newStart = edits[j].newIdx; break; }
          }
          if (newStart === -1) newStart = newLines.length;
        }
        oldCount++;
        hunkLines.push(`-${oldLines[edit.oldIdx]}`);
      } else if (edit.type === 'insert') {
        if (newStart === -1) newStart = edit.newIdx;
        if (oldStart === -1) {
          // Find next equal or delete to determine oldStart
          for (let j = i + 1; j <= endEdit; j++) {
            if (edits[j].type === 'equal') { oldStart = edits[j].oldIdx; break; }
            if (edits[j].type === 'delete') { oldStart = edits[j].oldIdx; break; }
          }
          if (oldStart === -1) oldStart = oldLines.length;
        }
        newCount++;
        hunkLines.push(`+${newLines[edit.newIdx]}`);
      }
    }

    // Convert to 1-based line numbers
    // When count is 0, start line is the anchor (line before change point)
    const oldStartLine = oldCount === 0 ? oldStart : oldStart + 1;
    const newStartLine = newCount === 0 ? newStart : newStart + 1;

    output.push(`@@ -${oldStartLine},${oldCount} +${newStartLine},${newCount} @@`);
    output.push(...hunkLines);
  }

  return output;
}

/**
 * RPC methods exposed to main thread
 */
const methods = {
  /**
   * Clone a repository (shallow by default for browsing)
   * @param {Object} options - Clone options
   * @param {string} options.url - Repository URL
   * @param {string} options.dir - Local directory path
   * @param {number} [options.depth=1] - Clone depth (shallow clone)
   * @param {number} [options.timeout=60000] - Timeout in milliseconds (default 60s)
   */
  async clone({ url, dir, depth = 1, timeout = 60000 }) {
    // Validate and normalize directory path to prevent path traversal attacks
    dir = validateRepoDir(dir);

    console.log(`[GitWorker] Cloning ${url} to ${dir} (depth: ${depth}, timeout: ${timeout}ms)`);

    // Set up abort controller for timeout
    const controller = new AbortController();
    const timeoutId = setTimeout(() => {
      console.warn(`[GitWorker] Clone timed out after ${timeout}ms`);
      controller.abort();
    }, timeout);

    try {
      await git.clone({
        fs,
        http,
        dir,
        url,
        corsProxy: getCorsProxy(url),
        depth,
        singleBranch: true,
        signal: controller.signal,
        onProgress: (progress) => {
          self.postMessage({
            type: 'progress',
            phase: progress.phase,
            loaded: progress.loaded,
            total: progress.total,
          });
        },
      });

      console.log(`[GitWorker] Clone complete: ${dir}`);
      return { success: true };
    } catch (e) {
      if (e.name === 'AbortError') {
        throw new Error(`Clone operation timed out after ${timeout / 1000}s`);
      }
      throw e;
    } finally {
      clearTimeout(timeoutId);
    }
  },

  /**
   * List files in a directory at a given ref
   */
  async listFiles({ dir, ref = 'HEAD', path = '' }) {
    // Validate and normalize directory path to prevent path traversal attacks
    dir = validateRepoDir(dir);

    // Resolve ref to commit OID (strict - no fallback for user-specified refs)
    let commitOid;
    try {
      commitOid = await git.resolveRef({ fs, dir, ref });
    } catch (e) {
      throw new Error(`Could not resolve ref '${ref}': ${e.message}`);
    }

    // Get all files at the resolved commit
    const files = await git.listFiles({ fs, dir, ref: commitOid });

    // Filter to path prefix and get immediate children
    const prefix = path ? `${path}/` : '';
    const entries = new Map();

    for (const file of files) {
      // Skip files not under our path
      if (path && !file.startsWith(prefix)) continue;

      // Get relative path from our directory
      const relative = path ? file.slice(prefix.length) : file;
      const parts = relative.split('/');
      const name = parts[0];
      const isDir = parts.length > 1;

      // Only add each name once (directories may appear multiple times)
      if (!entries.has(name)) {
        entries.set(name, {
          name,
          type: isDir ? 'tree' : 'blob',
          path: path ? `${path}/${name}` : name,
        });
      }
    }

    // Sort: directories first, then alphabetically
    return Array.from(entries.values()).sort((a, b) => {
      if (a.type !== b.type) return a.type === 'tree' ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
  },

  /**
   * Read file content at a given ref
   */
  async readFile({ dir, ref = 'HEAD', filepath }) {
    // Validate and normalize directory path to prevent path traversal attacks
    dir = validateRepoDir(dir);

    if (typeof filepath !== 'string') throw new Error('Invalid filepath type: ' + typeof filepath);
    // Reject filepaths containing control characters before sanitization
    if (filepath.split('').some(c => { const code = c.charCodeAt(0); return code <= 0x1F || code === 0x7F; })) {
      throw new Error(`Filepath contains control characters: ${sanitizeFilepath(filepath)}`);
    }

    let commitOid;
    try {
      commitOid = await git.resolveRef({ fs, dir, ref });
    } catch (e) {
      throw new Error(`Could not resolve ref '${ref}': ${e.message}`);
    }

    try {
      const { blob } = await git.readBlob({
        fs,
        dir,
        oid: commitOid,
        filepath: sanitizeFilepath(filepath),
      });
      return new TextDecoder().decode(blob);
    } catch (e) {
      throw new Error(`Could not read file '${sanitizeFilepath(filepath)}': ${e.message}`);
    }
  },

  /**
   * Get commit log
   */
  async log({ dir, ref = 'HEAD', depth = 50 }) {
    // Validate and normalize directory path to prevent path traversal attacks
    dir = validateRepoDir(dir);

    let commits;
    try {
      commits = await git.log({ fs, dir, ref, depth });
    } catch (e) {
      throw new Error(`Could not get log for ref '${ref}': ${e.message}`);
    }
    return commits.map((c) => ({
      oid: c.oid,
      message: c.commit.message,
      author: c.commit.author.name,
      email: c.commit.author.email,
      timestamp: c.commit.author.timestamp,
      parent: c.commit.parent[0] || null,
    }));
  },

  /**
   * List branches
   */
  async branches({ dir }) {
    // Validate and normalize directory path to prevent path traversal attacks
    dir = validateRepoDir(dir);
    return await git.listBranches({ fs, dir });
  },

  /**
   * Get current branch
   */
  async currentBranch({ dir }) {
    // Validate and normalize directory path to prevent path traversal attacks
    dir = validateRepoDir(dir);
    return await git.currentBranch({ fs, dir });
  },

  /**
   * Check if repo exists in cache
   */
  async status({ dir }) {
    // Validate and normalize directory path to prevent path traversal attacks
    dir = validateRepoDir(dir);
    try {
      await git.currentBranch({ fs, dir });
      return { exists: true };
    } catch {
      return { exists: false };
    }
  },

  /**
   * List all file paths recursively (flat list for fuzzy finder)
   */
  async listAllPaths({ dir, ref: gitRef = 'HEAD' }) {
    // Validate and normalize directory path to prevent path traversal attacks
    dir = validateRepoDir(dir);

    let commitOid;
    try {
      if (gitRef === 'HEAD') {
        commitOid = await resolveRefWithFallback(dir, gitRef);
      } else {
        commitOid = await git.resolveRef({ fs, dir, ref: gitRef });
      }
    } catch (e) {
      throw new Error(`Could not resolve ref '${gitRef}': ${e.message}`);
    }

    const files = await git.listFiles({ fs, dir, ref: commitOid });
    return files;
  },

  /**
   * Compare two refs and generate a unified diff
   * Walks both trees and compares file contents
   */
  async diff({ dir, base, head }) {
    dir = validateRepoDir(dir);

    if (!base || typeof base !== 'string') {
      throw new Error('Invalid base ref: must be a non-empty string');
    }
    if (!head || typeof head !== 'string') {
      throw new Error('Invalid head ref: must be a non-empty string');
    }

    // Maximum blob size to include in diff (2 MB)
    const MAX_DIFF_BYTES = 2 * 1024 * 1024;

    let baseOid, headOid;
    try {
      baseOid = await git.resolveRef({ fs, dir, ref: base });
    } catch (e) {
      throw new Error(`Could not resolve base ref '${base}': ${e.message}`);
    }
    try {
      headOid = await git.resolveRef({ fs, dir, ref: head });
    } catch (e) {
      throw new Error(`Could not resolve head ref '${head}': ${e.message}`);
    }

    // Get file listings for both refs (use Sets for O(1) lookup)
    const baseFilesList = await git.listFiles({ fs, dir, ref: baseOid });
    const headFilesList = await git.listFiles({ fs, dir, ref: headOid });
    const baseFiles = new Set(baseFilesList);
    const headFiles = new Set(headFilesList);

    const allFiles = new Set([...baseFiles, ...headFiles]);
    const diffParts = [];

    for (const filepath of allFiles) {
      const safePath = sanitizeFilepath(filepath);
      let baseContent = null;
      let headContent = null;
      let baseSkipReason = null;
      let headSkipReason = null;
      const inBase = baseFiles.has(filepath);
      const inHead = headFiles.has(filepath);

      if (inBase) {
        try {
          const { blob } = await git.readBlob({ fs, dir, oid: baseOid, filepath });
          if (blob.byteLength > MAX_DIFF_BYTES) {
            baseSkipReason = 'File too large to diff';
          } else if (isBinary(blob)) {
            baseSkipReason = 'Binary file';
          } else {
            baseContent = new TextDecoder().decode(blob);
          }
        } catch (e) {
          console.warn(`[GitWorker] Could not read base blob for '${filepath}': ${e.message}`, e);
          baseSkipReason = `Could not read base blob: ${e.message}`;
        }
      }

      if (inHead) {
        try {
          const { blob } = await git.readBlob({ fs, dir, oid: headOid, filepath });
          if (blob.byteLength > MAX_DIFF_BYTES) {
            headSkipReason = 'File too large to diff';
          } else if (isBinary(blob)) {
            headSkipReason = 'Binary file';
          } else {
            headContent = new TextDecoder().decode(blob);
          }
        } catch (e) {
          console.warn(`[GitWorker] Could not read head blob for '${filepath}': ${e.message}`, e);
          headSkipReason = `Could not read head blob: ${e.message}`;
        }
      }

      if (baseSkipReason && headSkipReason) {
        // If file exists in both refs with same skip reason, it's unchanged - skip it
        if (inBase && inHead && baseSkipReason === headSkipReason) continue;
        diffParts.push(`diff --git a/${safePath} b/${safePath}`);
        diffParts.push(`--- a/${safePath}`);
        diffParts.push(`+++ b/${safePath}`);
        diffParts.push(`File skipped: ${baseSkipReason}`);
        continue;
      }
      if (baseSkipReason || headSkipReason) {
        diffParts.push(`diff --git a/${safePath} b/${safePath}`);
        diffParts.push(inBase ? `--- a/${safePath}` : `--- /dev/null`);
        diffParts.push(inHead ? `+++ b/${safePath}` : `+++ /dev/null`);
        if (baseSkipReason) diffParts.push(`Base file skipped: ${baseSkipReason}`);
        if (headSkipReason) diffParts.push(`Head file skipped: ${headSkipReason}`);
        continue;
      }

      // Skip unchanged files
      if (baseContent === headContent) continue;

      // Generate unified diff header
      if (baseContent === null) {
        // New file
        const lines = normalizeAndSplit(headContent);
        diffParts.push(`diff --git a/${safePath} b/${safePath}`);
        diffParts.push('new file mode 100644');
        diffParts.push(`--- /dev/null`);
        diffParts.push(`+++ b/${safePath}`);
        diffParts.push(`@@ -0,0 +1,${lines.length} @@`);
        for (const line of lines) {
          diffParts.push(`+${line}`);
        }
      } else if (headContent === null) {
        // Deleted file
        const lines = normalizeAndSplit(baseContent);
        diffParts.push(`diff --git a/${safePath} b/${safePath}`);
        diffParts.push('deleted file mode 100644');
        diffParts.push(`--- a/${safePath}`);
        diffParts.push(`+++ /dev/null`);
        diffParts.push(`@@ -1,${lines.length} +0,0 @@`);
        for (const line of lines) {
          diffParts.push(`-${line}`);
        }
      } else {
        // Modified file - proper unified diff with LCS algorithm
        const baseLines = normalizeAndSplit(baseContent);
        const headLines = normalizeAndSplit(headContent);

        // Compute edit script using LCS (Myers-like O(ND) diff)
        const edits = computeEdits(baseLines, headLines);
        // Generate unified diff hunks with 3 lines of context
        const hunks = generateHunks(edits, baseLines, headLines, 3);
        if (hunks.length > 0) {
          diffParts.push(`diff --git a/${safePath} b/${safePath}`);
          diffParts.push(`--- a/${safePath}`);
          diffParts.push(`+++ b/${safePath}`);
          for (const hunk of hunks) {
            diffParts.push(hunk);
          }
        }
      }
    }

    return diffParts.join('\n');
  },

  /**
   * Delete a cached repo
   */
  async deleteRepo({ dir }) {
    // Validate and normalize directory path to prevent path traversal attacks
    try {
      dir = validateRepoDir(dir);
    } catch (e) {
      return { success: false, error: e.message };
    }

    try {
      // Recursively delete directory
      const deleteRecursive = async (path) => {
        const stat = await fs.promises.stat(path).catch(() => null);
        if (!stat) return;

        if (stat.isDirectory()) {
          const files = await fs.promises.readdir(path);
          for (const file of files) {
            await deleteRecursive(`${path}/${file}`);
          }
          await fs.promises.rmdir(path);
        } else {
          await fs.promises.unlink(path);
        }
      };

      await deleteRecursive(dir);
      return { success: true };
    } catch (error) {
      return { success: false, error: error.message };
    }
  },
};

// RPC message handler
self.onmessage = async (e) => {
  const data = e.data;

  // Handle GRASP server updates (non-RPC message)
  if (data.type === 'updateGraspServers' && Array.isArray(data.servers)) {
    // Hostname validation regex: domain labels separated by dots
    const hostnameRegex = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*$/i;
    data.servers.forEach(server => {
      if (server && typeof server === 'string' && hostnameRegex.test(server)) {
        graspServers.add(server);
      } else {
        console.warn('[GitWorker] Invalid GRASP server ignored:', server);
      }
    });
    console.log('[GitWorker] Updated GRASP servers:', [...graspServers]);
    return;
  }

  // Standard RPC handling
  const { id, method, params } = data;

  try {
    if (!methods[method]) {
      throw new Error(`Unknown method: ${method}`);
    }

    const result = await methods[method](params);
    self.postMessage({ id, type: 'result', result });
  } catch (error) {
    console.error(`[GitWorker] Error in ${method}:`, error);
    self.postMessage({ id, type: 'error', error: error.message });
  }
};

// Signal that worker is ready
self.postMessage({ type: 'ready' });
console.log('[GitWorker] Worker initialized');
