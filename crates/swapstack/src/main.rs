use swapstack::coroutine::Coroutine;

fn main() {
    let mut a = 0;
    let f = Coroutine::new(|f| {
        a = 42;

        f.resume()
    });

    let x = f.resume();
    println!("{}", a);
    drop(x);
}
