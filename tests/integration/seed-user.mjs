// Emits an idempotent SQL INSERT for a bootstrap operator user, so the CLI can
// log in against a fresh local backend (there is intentionally no seed endpoint).
//
// Run from the BACKEND directory so `@noble/hashes` resolves from its node_modules.
// The Argon2id PHC and the HMAC blind index are computed EXACTLY as the backend
// does (src/auth/password.ts, src/security/blind-index.ts, src/lib/encoding.ts),
// so login lookup + verify succeed. Uses light Argon2 params (verify reads cost
// from the PHC, so this is fine and fast for a dev seed).
//
//   node seed-user.mjs <email> <password> [role] [id] > seed.sql
// Requires env BLIND_INDEX_KEY (must match the running worker's .dev.vars).

import { argon2idAsync } from '@noble/hashes/argon2.js'

const [email, password, role = 'developer', id = crypto.randomUUID()] = process.argv.slice(2)
const blindKey = process.env.BLIND_INDEX_KEY

if (!email || !password) {
  console.error('usage: node seed-user.mjs <email> <password> [role] [id]')
  process.exit(2)
}
if (!blindKey) {
  console.error('BLIND_INDEX_KEY env is required (must match the worker .dev.vars)')
  process.exit(2)
}

const b64NoPad = (bytes) => Buffer.from(bytes).toString('base64').replace(/=+$/, '')
const b64Url = (bytes) => b64NoPad(bytes).replace(/\+/g, '-').replace(/\//g, '_')

// HMAC-SHA-256 blind index over trim().toLowerCase(), base64url no pad.
async function blindIndex(key, value) {
  const ck = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(key),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  )
  const sig = await crypto.subtle.sign('HMAC', ck, new TextEncoder().encode(value.trim().toLowerCase()))
  return b64Url(new Uint8Array(sig))
}

// Argon2id PHC string, matching the backend's format exactly.
async function hashPassword(plain) {
  const m = 512, t = 1, p = 1
  const salt = crypto.getRandomValues(new Uint8Array(16))
  const hash = await argon2idAsync(new TextEncoder().encode(plain), salt, { m, t, p, dkLen: 32 })
  return `$argon2id$v=19$m=${m},t=${t},p=${p}$${b64NoPad(salt)}$${b64NoPad(hash)}`
}

const bidx = await blindIndex(blindKey, email)
const phc = await hashPassword(password)
const now = Date.now()

// email_enc is NOT NULL but never decrypted on the login path → a placeholder
// blob is sufficient for a bootstrap operator. INSERT OR IGNORE makes re-runs safe.
const sql = `INSERT OR IGNORE INTO users
  (id, role, tenant_id, email_enc, email_bidx, password_hash, status, must_change_password, created_at, updated_at)
  VALUES ('${id}', '${role}', NULL, X'00', '${bidx}', '${phc}', 'active', 0, ${now}, ${now});`

process.stdout.write(sql + '\n')
