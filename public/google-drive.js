/**
 * nostr.blue Google Drive Backup Bridge
 *
 * Provides encrypted private key backup to Google Drive appDataFolder.
 * Uses Google Identity Services (GIS) for OAuth and Drive REST v3 for storage.
 *
 * Setup: https://console.cloud.google.com/
 * 1. Create OAuth 2.0 Client ID (Web application)
 * 2. Add authorized origins for your domain + localhost:8080
 * 3. Enable Google Drive API
 * 4. Configure OAuth consent screen with drive.appdata scope
 */
window.nostrBlueDrive = {
  CLIENT_ID:
    "665414552910-b0b9mu4guac4bk9hdoc751uqqmd6irum.apps.googleusercontent.com",
  SCOPE: "https://www.googleapis.com/auth/drive.appdata openid",
  DRIVE_FILES_URL: "https://www.googleapis.com/drive/v3/files",
  TOKENINFO_URL: "https://oauth2.googleapis.com/tokeninfo",
  BACKUP_PREFIX: "nostrblue_backup_",
  BACKUP_SUFFIX: ".bin",
  GIS_URL: "https://accounts.google.com/gsi/client",

  tokenClient: null,
  _gisLoaded: false,

  async _loadGIS() {
    if (this._gisLoaded) return;
    return new Promise((resolve, reject) => {
      const script = document.createElement("script");
      script.src = this.GIS_URL;
      script.onload = () => {
        this._gisLoaded = true;
        resolve();
      };
      script.onerror = () => reject(new Error("Failed to load Google Identity Services"));
      document.head.appendChild(script);
    });
  },

  async _initTokenClient() {
    await this._loadGIS();
    if (this.tokenClient) return;
    this.tokenClient = google.accounts.oauth2.initTokenClient({
      client_id: this.CLIENT_ID,
      scope: this.SCOPE,
      callback: "",
    });
  },

  async _getAccessToken() {
    await this._initTokenClient();
    return new Promise((resolve, reject) => {
      this.tokenClient.callback = (resp) => {
        if (resp.error) {
          reject(new Error(resp.error));
          return;
        }
        resolve(resp.access_token);
      };
      this.tokenClient.requestAccessToken({ prompt: "consent" });
    });
  },

  async signIn() {
    const accessToken = await this._getAccessToken();
    const resp = await fetch(this.TOKENINFO_URL, {
      headers: { Authorization: `Bearer ${accessToken}` },
    });
    if (!resp.ok) throw new Error("Failed to get token info");
    const info = await resp.json();
    if (!info.sub) throw new Error("No sub claim in token info");
    return { sub: info.sub, accessToken: accessToken };
  },

  async list(accessToken) {
    const query = `name contains '${this.BACKUP_PREFIX}'`;
    const url = `${this.DRIVE_FILES_URL}?spaces=appDataFolder&q=${encodeURIComponent(query)}&fields=files(id,name,modifiedTime)&pageSize=100`;
    const resp = await fetch(url, {
      headers: { Authorization: `Bearer ${accessToken}` },
    });
    if (!resp.ok) throw new Error(`Drive list failed: ${resp.status}`);
    const data = await resp.json();
    return (data.files || []).map((f) => ({ fileId: f.id, name: f.name }));
  },

  async upload(accessToken, npub, payload) {
    const filename = this.BACKUP_PREFIX + npub + this.BACKUP_SUFFIX;

    let oldFileIds = [];
    try {
      const existing = await this.list(accessToken);
      oldFileIds = existing
        .filter((f) => f.name === filename)
        .map((f) => f.fileId);
    } catch (e) {
      console.warn("[nostrBlueDrive] Failed to list existing backups:", e);
    }

    const metadata = JSON.stringify({
      name: filename,
      parents: ["appDataFolder"],
    });

    // Step 1: Start resumable upload session
    const initResp = await fetch(
      "https://www.googleapis.com/upload/drive/v3/files?uploadType=resumable",
      {
        method: "POST",
        headers: {
          Authorization: `Bearer ${accessToken}`,
          "Content-Type": "application/json; charset=UTF-8",
          "X-Upload-Content-Type": "application/octet-stream",
        },
        body: metadata,
      }
    );
    if (!initResp.ok) {
      const text = await initResp.text();
      throw new Error(`Drive upload init failed: ${initResp.status} ${text}`);
    }

    const uploadUrl = initResp.headers.get("Location");
    if (!uploadUrl) {
      throw new Error("Drive upload init returned no Location header");
    }

    // Step 2: Upload content to the resumable URL
    const resp = await fetch(uploadUrl, {
      method: "PUT",
      headers: {
        "Content-Type": "application/octet-stream",
      },
      body: payload,
    });
    if (!resp.ok) {
      const text = await resp.text();
      throw new Error(`Drive upload failed: ${resp.status} ${text}`);
    }

    for (const oldId of oldFileIds) {
      try {
        await this._rawDelete(accessToken, oldId);
      } catch (e) {
        console.warn("[nostrBlueDrive] Failed to delete old backup:", e);
      }
    }
  },

  async download(accessToken, fileId) {
    const url = `${this.DRIVE_FILES_URL}/${fileId}?alt=media`;
    const resp = await fetch(url, {
      headers: { Authorization: `Bearer ${accessToken}` },
    });
    if (!resp.ok) throw new Error(`Drive download failed: ${resp.status}`);
    return await resp.text();
  },

  async delete(accessToken, fileId) {
    await this._rawDelete(accessToken, fileId);
  },

  async _rawDelete(accessToken, fileId) {
    const url = `${this.DRIVE_FILES_URL}/${fileId}`;
    const resp = await fetch(url, {
      method: "DELETE",
      headers: { Authorization: `Bearer ${accessToken}` },
    });
    if (!resp.ok) {
      throw new Error(`Delete failed: ${resp.status}`);
    }
  },
};
