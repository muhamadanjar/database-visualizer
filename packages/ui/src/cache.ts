import type { Row } from '@database-studio/contracts';
export class ResultCache {
  private entries = new Map<
    string,
    { owner: string; rows: Row[]; bytes: number }
  >();
  private bytes = 0;
  constructor(
    private perTab = 32 * 1024 * 1024,
    private total = 128 * 1024 * 1024,
  ) {}
  get(key: string) {
    const value = this.entries.get(key);
    if (value) {
      this.entries.delete(key);
      this.entries.set(key, value);
    }
    return value?.rows;
  }
  put(key: string, owner: string, rows: Row[]) {
    this.delete(key);
    const bytes = new TextEncoder().encode(JSON.stringify(rows)).byteLength;
    if (bytes > this.perTab) return;
    this.entries.set(key, { owner, rows, bytes });
    this.bytes += bytes;
    while (this.bytes > this.total || this.ownerBytes(owner) > this.perTab) {
      const candidate = [...this.entries].find(
        ([, v]) => this.bytes > this.total || v.owner === owner,
      );
      if (!candidate) break;
      this.delete(candidate[0]);
    }
  }
  private ownerBytes(owner: string) {
    let bytes = 0;
    for (const value of this.entries.values())
      if (value.owner === owner) bytes += value.bytes;
    return bytes;
  }
  private delete(key: string) {
    const entry = this.entries.get(key);
    if (entry) {
      this.bytes -= entry.bytes;
      this.entries.delete(key);
    }
  }
  clear(owner: string) {
    for (const [key, value] of this.entries)
      if (value.owner === owner) this.delete(key);
  }
  size() {
    return this.bytes;
  }
}
export const resultCache = new ResultCache();
