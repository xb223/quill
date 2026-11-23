struct Point {
    x: int,
    y: int,
}

fn distance_squared(a: Point, b: Point) -> int {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn fib(n: int) -> int {
    if n < 2 {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}

fn greet(name: str, points: int) -> str {
    "hello, " + name + " (" + str(points) + " points)"
}

fn main() {
    let origin = Point { x: 0, y: 0 };
    let p = Point { x: 3, y: 4 };
    println("distance² = {distance_squared(origin, p)}");

    let mut total = 0;
    for i in 1..6 {
        total = total + i;
    }
    println("1 + 2 + 3 + 4 + 5 = {total}");

    let xs = [1, 2, 3];
    push(xs, 4);
    push(xs, 5);
    println("array:");
    println(xs);
    println("length: {len(xs)}");

    println("fib(10) = {fib(10)}");
    println(greet("world", 42));

    let f: float = 3;
    println("float division: {f / 2.0}");
    println("float formatting: " + str(1.5) + ", " + str(1.0));

    if total == 15 && len(xs) == 5 {
        println("all checks passed");
    } else {
        println("something is off");
    }
}
