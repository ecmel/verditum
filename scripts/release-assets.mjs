/*
 * Collects the release workflow's artifacts, one directory per job, into a
 * flat directory of release assets and writes the desktop updater manifest,
 * latest.json, next to them. GITHUB_REPOSITORY names the repository whose
 * release will host the assets.
 *
 * Usage: node scripts/release-assets.mjs <artifacts> <output> <tag>
 */

import fs from "fs/promises";
import path from "path";

const [artifacts, output, tag] = process.argv.slice(2);
const repository = process.env.GITHUB_REPOSITORY;
if (!artifacts || !output || !tag || !repository) {
  throw new Error(
    "Usage: GITHUB_REPOSITORY=<owner/name> node scripts/release-assets.mjs <artifacts> <output> <tag>",
  );
}

// Updater platform keys, `{os}-{arch}`, of the desktop artifacts by job name.
const platforms = {
  "aarch64-apple-darwin": "darwin-aarch64",
  "x86_64-apple-darwin": "darwin-x86_64",
  "x86_64-pc-windows-msvc": "windows-x86_64",
  "aarch64-pc-windows-msvc": "windows-aarch64",
  "x86_64-unknown-linux-gnu": "linux-x86_64",
  "aarch64-unknown-linux-gnu": "linux-aarch64",
};

// Installed apps compare their own version with the manifest's, so a tag that
// differs from the built version would offer the same update forever.
const { version } = JSON.parse(
  await fs.readFile("src-tauri/tauri.conf.json", "utf-8"),
);
if (tag !== `v${version}`) {
  throw new Error(`Tag ${tag} does not match version ${version}`);
}

async function files(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map((entry) => {
      const file = path.join(dir, entry.name);
      return entry.isDirectory() ? files(file) : [file];
    }),
  );
  return nested.flat();
}

async function copy(file, name) {
  // Fails instead of letting one job's asset replace another's.
  await fs.copyFile(file, path.join(output, name), fs.constants.COPYFILE_EXCL);
}

await fs.mkdir(output, { recursive: true });

const manifest = {
  version,
  pub_date: new Date().toISOString(),
  platforms: {},
};

for (const job of await fs.readdir(artifacts)) {
  const all = await files(path.join(artifacts, job));
  const signatures = all.filter((file) => file.endsWith(".sig"));
  const updates = signatures.map((file) => file.slice(0, -".sig".length));
  for (const file of all) {
    if (!signatures.includes(file) && !updates.includes(file)) {
      await copy(file, path.basename(file));
    }
  }

  const platform = platforms[job];
  if (!platform && !updates.length) continue;
  if (!platform || updates.length !== 1) {
    throw new Error(`Expected one updater artifact per desktop job: ${job}`);
  }

  let name = path.basename(updates[0]);
  // macOS archives carry only the app name, the same for every architecture.
  if (name.endsWith(".app.tar.gz")) {
    const arch = platform.split("-")[1];
    name = `${name.slice(0, -".app.tar.gz".length)}_${version}_${arch}.app.tar.gz`;
  }
  await copy(updates[0], name);
  await copy(signatures[0], `${name}.sig`);

  manifest.platforms[platform] = {
    signature: (await fs.readFile(signatures[0], "utf-8")).trim(),
    url: `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(name)}`,
  };
}

const missing = Object.values(platforms).filter(
  (platform) => !manifest.platforms[platform],
);
if (missing.length) {
  throw new Error(`No updater artifact for ${missing.join(", ")}`);
}

await fs.writeFile(
  path.join(output, "latest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
