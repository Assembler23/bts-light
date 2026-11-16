// Prüft eine minisign-Signatur in der Schreibweise des Tauri-Signierwerkzeugs
// gegen den Updater-Schlüssel aus src-tauri/tauri.conf.json.
//
// Wofür: Der Release-Job `kern` entnimmt die Programmdatei dem
// veröffentlichten Installer und signiert einen Kopf für sie. Davor muss
// feststehen, dass der Installer wirklich der ist, den der Build-Job
// signiert hat — sonst würde der Job für eine ausgetauschte Datei eine echte
// Signatur ausstellen (Security-Review 16.11.2026).
//
// Nur die Form mit Vorab-Hash (BLAKE2b-512), wie in der App (signatur.rs).
//
// Aufruf: node scripts/verify-minisign.mjs <datei> <datei.sig>
import { createHash, createPublicKey, verify } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function b64(text) {
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(text)) throw new Error("kein Base64");
  return Buffer.from(text, "base64");
}

/** Der rohe Ed25519-Schlüssel als Schlüsselobjekt. */
function ed25519(roh) {
  // SubjectPublicKeyInfo für Ed25519: fester Vorspann + 32 Bytes Schlüssel.
  const spki = Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), roh]);
  return createPublicKey({ key: spki, format: "der", type: "spki" });
}

/** Liest einen Schlüssel, wie er in tauri.conf.json steht. */
export function schluesselLesen(kodiert) {
  const zeilen = b64(kodiert.trim()).toString("utf8").split("\n");
  const bin = b64((zeilen[1] ?? "").trim());
  if (bin.length !== 42 || bin[0] !== 0x45) throw new Error("Schlüssel unlesbar");
  return { kennung: bin.subarray(2, 10), schluessel: ed25519(bin.subarray(10, 42)) };
}

/** Der Updater-Schlüssel dieser App. */
export function updaterSchluessel() {
  const conf = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
  return schluesselLesen(conf.plugins.updater.pubkey);
}

/**
 * Prüft `signatur` (Inhalt der `.sig`-Datei) über `daten`. Wirft bei jedem
 * Mangel; kehrt nur zurück, wenn die Signatur gilt.
 */
export function pruefen({ kennung, schluessel }, daten, signatur) {
  const zeilen = b64(signatur.trim()).toString("utf8").split("\n");
  const bin = b64((zeilen[1] ?? "").trim());
  const kommentar = zeilen[2] ?? "";
  const gesamt = b64((zeilen[3] ?? "").trim());
  if (bin.length !== 74 || gesamt.length !== 64 || !kommentar.startsWith("trusted comment: ")) {
    throw new Error("Signatur unlesbar");
  }
  if (bin[0] !== 0x45 || bin[1] !== 0x44) {
    throw new Error("Signatur ohne Vorab-Hash wird nicht angenommen");
  }
  if (!bin.subarray(2, 10).equals(kennung)) throw new Error("Signatur von einem anderen Schlüssel");
  const sig = bin.subarray(10, 74);
  const hash = createHash("blake2b512").update(daten).digest();
  if (!verify(null, hash, schluessel, sig)) throw new Error("Signatur passt nicht zu den Daten");
  const text = Buffer.from(kommentar.slice("trusted comment: ".length), "utf8");
  if (!verify(null, Buffer.concat([sig, text]), schluessel, gesamt)) {
    throw new Error("Signatur-Anhang passt nicht");
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const [datei, sig] = process.argv.slice(2);
    if (!datei || !sig) throw new Error("Aufruf: verify-minisign.mjs <datei> <datei.sig>");
    pruefen(updaterSchluessel(), readFileSync(datei), readFileSync(sig, "utf8"));
    console.error(`${datei}: Signatur des Updater-Schlüssels gilt.`);
  } catch (e) {
    console.error(`Fehler: ${e.message}`);
    process.exit(1);
  }
}
