export class Cart {
    private items: string[] = [];

    add(item: string, count = 1): void {
        for (let i = 0; i < count; i++) {
            this.items.push(item);
        }
    }

    get size(): number {
        return this.items.length;
    }
}
