fn main() {
    let xs: [int] = [];
    for i in 0..3 {
        push(xs, i * i);
    }
    println(xs);
    println(xs[2]);

    let mut best = 0;
    for value in xs {
        if value > best {
            best = value;
        }
    }
    println("best = {best}");

    let names = ["ada", "grace", "alan"];
    for name in names {
        println("hello, {name}");
    }

    let scores: [float] = [1, 2, 3];
    println(scores);
    println("average-ish: {scores[0] / 2.0}");
}
