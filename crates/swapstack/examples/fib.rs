use swapstack::coroutine::*;

fn main() {
    let mut a = 0;

    let mut f = Coroutine::new(|mut f| {
        a = 0;
        let mut b = 1;

        loop {
            f = f.resume();
            let next = a + b;
            a = b;
            b = next;
        }
    });

    for _ in 0..10 {
        f = f.resume();
        println!("{}", a);
    }
}
