// Git Web Worker using isomorphic-git
// Handles git operations off the main thread for nostr.blue

import git from 'isomorphic-git';
import http from 'isomorphic-git/http/web';
import LightningFS from '@isomorphic-git/lightning-fs';
import { Buffer } from 'buffer';

// Required for isomorphic-git
globalThis.Buffer = Buffer;

// Initialize virtual filesystem backed by IndexedDB
const fs = new LightningFS('nostr-blue-git-cache');

// CORS proxy for GitHub/GitLab/etc
const CORS_PROXY = 'https://cors.isomorphic-git.org';

/**
 * Resolve a git ref with fallback to default branches
 * First tries the requested ref, then falls back to main/master local branches,
 * then tries remote branches if no local branches exist.
 */
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
  if (!dir || dir.includes('..') || dir.startsWith('/') || dir.includes('\0')) {
    throw new Error('Invalid directory path');
  }
  const normalized = dir.split('/').filter(p => p && p !== '.').join('/');
  if (normalized !== dir || normalized.includes('..')) {
    throw new Error('Invalid directory path');
  }
  return normalized;
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
    // Validate directory path to prevent path traversal attacks
    validateRepoDir(dir);

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
    // Validate directory path to prevent path traversal attacks
    validateRepoDir(dir);

    // Resolve ref to commit OID with fallback
    let commitOid;
    try {
      commitOid = await resolveRefWithFallback(dir, ref);
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
    // Validate directory path to prevent path traversal attacks
    validateRepoDir(dir);

    let commitOid;
    try {
      commitOid = await resolveRefWithFallback(dir, ref);
    } catch (e) {
      throw new Error(`Could not resolve ref '${ref}': ${e.message}`);
    }

    try {
      const { blob } = await git.readBlob({
        fs,
        dir,
        oid: commitOid,
        filepath,
      });
      return new TextDecoder().decode(blob);
    } catch (e) {
      throw new Error(`Could not read file '${filepath}': ${e.message}`);
    }
  },

  /**
   * Get commit log
   */
  async log({ dir, ref = 'HEAD', depth = 50 }) {
    // Validate directory path to prevent path traversal attacks
    validateRepoDir(dir);

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
    // Validate directory path to prevent path traversal attacks
    validateRepoDir(dir);
    return await git.listBranches({ fs, dir });
  },

  /**
   * Get current branch
   */
  async currentBranch({ dir }) {
    // Validate directory path to prevent path traversal attacks
    validateRepoDir(dir);
    return await git.currentBranch({ fs, dir });
  },

  /**
   * Check if repo exists in cache
   */
  async status({ dir }) {
    // Validate directory path to prevent path traversal attacks
    validateRepoDir(dir);
    try {
      await git.currentBranch({ fs, dir });
      return { exists: true };
    } catch {
      return { exists: false };
    }
  },

  /**
   * Delete a cached repo
   */
  async deleteRepo({ dir }) {
    // Validate directory path to prevent path traversal attacks
    try {
      validateRepoDir(dir);
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
