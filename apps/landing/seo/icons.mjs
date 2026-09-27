import { access, copyFile, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

// Use the same SVGs as crates/ui/src/icons.rs, without redrawing their geometry.
export const nativeIcons = ['sun', 'moon', 'arrow-up-right', 'arrow-down', 'clock-circle', 'plus', 'star'];
export async function syncNativeIcons() {
  const source = fileURLToPath(new URL('../../../crates/ui/assets/icons/', import.meta.url));
  const target = fileURLToPath(new URL('../public/assets/icons/native/', import.meta.url));
  // Vercel deploys apps/landing alone; its checked-in mirror is the fallback.
  try { await access(source); }
  catch (error) {
    if (error.code !== 'ENOENT') throw error;
    await Promise.all(nativeIcons.map(name => access(resolve(target, `${name}.svg`))));
    return;
  }
  await mkdir(target, { recursive: true });
  await Promise.all(nativeIcons.map(name => copyFile(resolve(source, `${name}.svg`), resolve(target, `${name}.svg`))));
}
