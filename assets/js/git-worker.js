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

// Domains that need CORS proxy
const NEEDS_PROXY = ['github.com', 'gitlab.com', 'codeberg.org', 'gitea.com'];

// Known GRASP servers (have CORS enabled per spec)
const GRASP_SERVERS = [
  'relay.ngit.dev',
  'gitnostr.com',
  'ngit.danconwaydev.com',
  'git.shakespeare.diy',
  'git-01.uid.ovh',
  'git-02.uid.ovh',
  'git.jb55.com',
];

/**
 * Check if a URL needs CORS proxy
 */
function needsProxy(url) {
  // GRASP servers don't need proxy
  if (GRASP_SERVERS.some((s) => url.includes(s))) return false;
  // Known blocked domains need proxy
  return NEEDS_PROXY.some((s) => url.includes(s));
}

/**
 * Get CORS proxy URL if needed
 */
function getCorsProxy(url) {
  return needsProxy(url) ? CORS_PROXY : undefined;
}

/**
 * RPC methods exposed to main thread
 */
const methods = {
  /**
   * Clone a repository (shallow by default for browsing)
   */
  async clone({ url, dir, depth = 1 }) {
    console.log(`[GitWorker] Cloning ${url} to ${dir} (depth: ${depth})`);

    await git.clone({
      fs,
      http,
      dir,
      url,
      corsProxy: getCorsProxy(url),
      depth,
      singleBranch: true,
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
  },

  /**
   * List files in a directory at a given ref
   */
  async listFiles({ dir, ref = 'HEAD', path = '' }) {
    // Resolve ref to commit OID
    let commitOid;
    try {
      commitOid = await git.resolveRef({ fs, dir, ref });
    } catch (e) {
      // If HEAD resolution fails, try to find any available branch
      try {
        const branches = await git.listBranches({ fs, dir });
        if (branches.length > 0) {
          // Try common default branch names first
          const defaultBranch = branches.find(b => b === 'main')
            || branches.find(b => b === 'master')
            || branches[0];
          commitOid = await git.resolveRef({ fs, dir, ref: defaultBranch });
        } else {
          // No local branches, try remote branches
          const remoteBranches = await git.listBranches({ fs, dir, remote: 'origin' });
          if (remoteBranches.length > 0) {
            const defaultBranch = remoteBranches.find(b => b === 'main')
              || remoteBranches.find(b => b === 'master')
              || remoteBranches[0];
            commitOid = await git.resolveRef({ fs, dir, ref: `refs/remotes/origin/${defaultBranch}` });
          } else {
            throw new Error('No branches found in repository');
          }
        }
      } catch (e2) {
        throw new Error(`Could not resolve ref: ${e2.message || e.message}`);
      }
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
    return await git.listBranches({ fs, dir });
  },

  /**
   * Get current branch
   */
  async currentBranch({ dir }) {
    return await git.currentBranch({ fs, dir });
  },

  /**
   * Check if repo exists in cache
   */
  async status({ dir }) {
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
    // Validate dir parameter to prevent path traversal
    if (!dir || dir.includes('..') || dir.startsWith('/')) {
      return { success: false, error: 'Invalid directory path' };
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
  const { id, method, params } = e.data;

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
