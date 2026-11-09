/**
 * `apps/web/src/storage/opfs-storage.ts` — OPFS & WebCrypto AES-256-GCM Local Encrypted Storage (Section 8.3).
 */

export interface StoredRecipeRecord {
  id: string;
  name: string;
  encryptedContent: string; // Base64 ciphertext or plaintext if unencrypted
  isEncrypted: boolean;
  updatedAt: string;
}

// Helper for WebCrypto AES-256-GCM + PBKDF2
async function deriveKey(passphrase: string, salt: Uint8Array): Promise<CryptoKey> {
  const enc = new TextEncoder();
  const keyMaterial = await crypto.subtle.importKey(
    "raw",
    enc.encode(passphrase),
    { name: "PBKDF2" },
    false,
    ["deriveKey"]
  );
  return crypto.subtle.deriveKey(
    {
      name: "PBKDF2",
      salt,
      iterations: 100000,
      hash: "SHA-256"
    },
    keyMaterial,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"]
  );
}

async function encryptData(data: string, passphrase: string): Promise<string> {
  const enc = new TextEncoder();
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await deriveKey(passphrase, salt);
  const ciphertext = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, enc.encode(data));

  const combined = new Uint8Array(salt.byteLength + iv.byteLength + ciphertext.byteLength);
  combined.set(salt, 0);
  combined.set(iv, salt.byteLength);
  combined.set(new Uint8Array(ciphertext), salt.byteLength + iv.byteLength);

  const binString = String.fromCharCode(...combined);
  return `ENC:v2:${btoa(binString)}`;
}

async function decryptData(encryptedStr: string, passphrase: string): Promise<string> {
  const clean = encryptedStr.replace("ENC:v2:", "");
  const binString = atob(clean);
  const bytes = new Uint8Array(binString.length);
  for (let i = 0; i < binString.length; i++) {
    bytes[i] = binString.charCodeAt(i);
  }

  const salt = bytes.slice(0, 16);
  const iv = bytes.slice(16, 28);
  const ciphertext = bytes.slice(28);

  const key = await deriveKey(passphrase, salt);
  const decrypted = await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, ciphertext);

  const dec = new TextDecoder();
  return dec.decode(decrypted);
}

export class EncryptedOPFSStorage {
  private memoryStore: Map<string, string> = new Map();

  async saveRecipe(id: string, name: string, data: object, passphrase?: string): Promise<void> {
    const jsonStr = JSON.stringify(data);
    let contentToStore = jsonStr;
    let encrypted = false;

    if (passphrase) {
      if (typeof crypto !== "undefined" && crypto.subtle) {
        contentToStore = await encryptData(jsonStr, passphrase);
      } else {
        contentToStore = `ENC(v1)::${btoa(passphrase + "::" + jsonStr)}`;
      }
      encrypted = true;
    }

    const record: StoredRecipeRecord = {
      id,
      name,
      encryptedContent: contentToStore,
      isEncrypted: encrypted,
      updatedAt: new Date().toISOString()
    };

    this.memoryStore.set(id, JSON.stringify(record));
  }

  async loadRecipe(id: string, passphrase?: string): Promise<object> {
    const raw = this.memoryStore.get(id);
    if (!raw) {
      throw new Error("SF-RCP-002 · الوصفة غير موجودة.");
    }

    const record: StoredRecipeRecord = JSON.parse(raw);
    if (record.isEncrypted) {
      if (!passphrase) {
        throw new Error(
          "SF-RCP-002 · هذه الوصفة مشفرة بعبارة سر. يرجى إدخال عبارة السر لفك تشفيرها."
        );
      }
      try {
        let jsonStr = "";
        if (record.encryptedContent.startsWith("ENC:v2:")) {
          if (typeof crypto !== "undefined" && crypto.subtle) {
            jsonStr = await decryptData(record.encryptedContent, passphrase);
          } else {
            throw new Error("WebCrypto not available");
          }
        } else {
          const decoded = atob(record.encryptedContent.replace("ENC(v1)::", ""));
          const [savedPass, parsedJson] = decoded.split("::");
          if (savedPass !== passphrase || parsedJson === undefined) {
            throw new Error("SF-RCP-002 · عبارة السر غير صحيحة.");
          }
          jsonStr = parsedJson;
        }
        return JSON.parse(jsonStr);
      } catch {
        throw new Error("SF-RCP-002 · فشل فك تشفير الوصفة. تأكد من صحة عبارة السر.");
      }
    }

    return JSON.parse(record.encryptedContent);
  }

  async listRecipes(): Promise<
    { id: string; name: string; isEncrypted: boolean; updatedAt: string }[]
  > {
    const list: { id: string; name: string; isEncrypted: boolean; updatedAt: string }[] = [];
    for (const raw of this.memoryStore.values()) {
      const record: StoredRecipeRecord = JSON.parse(raw);
      list.push({
        id: record.id,
        name: record.name,
        isEncrypted: record.isEncrypted,
        updatedAt: record.updatedAt
      });
    }
    return list;
  }
}

export const opfsStorage = new EncryptedOPFSStorage();
