export class Cart {
    private items: string[] = [];

    add(item: string): void {
        this.items.push(item);
    }

    clear(): void {
        this.items = [];
    }

    get size(): number {
        return this.items.length;
    }
}
