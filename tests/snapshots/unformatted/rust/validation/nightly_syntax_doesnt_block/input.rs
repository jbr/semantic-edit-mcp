#![feature(gen_blocks)]

fn numbers() -> impl Iterator<Item = u32> {
    gen {
        yield 1;
        yield 2;
    }
}
