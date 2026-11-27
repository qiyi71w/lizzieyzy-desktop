/** A map of at most `limit` entries that makes room by dropping the entry used longest ago, so the
 *  entries every repaint reads stay while one-off ones come and go. */
export class RecentCache<K, V> {
  private readonly entries = new Map<K, V>();
  private readonly limit: number;

  constructor(limit: number) {
    this.limit = limit;
  }

  get size(): number {
    return this.entries.size;
  }

  get(key: K): V | undefined {
    const value = this.entries.get(key);
    if (value !== undefined) {
      // A Map iterates in insertion order: the entry moves to the most recent end.
      this.entries.delete(key);
      this.entries.set(key, value);
    }
    return value;
  }

  set(key: K, value: V): void {
    this.entries.delete(key);
    if (this.entries.size >= this.limit) this.entries.delete(this.entries.keys().next().value as K);
    this.entries.set(key, value);
  }
}
