export class Cart {
    private items: string[] = [];

    add(item: string): void {
        this.items.push(item);
    }

    get size(): number {
        return this.items.length;
    }
}
