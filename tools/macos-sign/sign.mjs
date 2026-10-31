import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

if (process.platform === 'darwin') {
  const manifest = fileURLToPath(new URL('./Cargo.toml', import.meta.url));
  const result = spawnSync('cargo', ['run', '--locked', '--manifest-path', manifest, '--', 'src-tauri/target/electron', 'src-tauri/resources'], { stdio: 'inherit' });
  if (result.error) throw result.error;
  process.exit(result.status ?? 1);
}
