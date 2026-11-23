fn classify(n: int) -> str {
    if n < 0 {
        "negative"
    } else if n == 0 {
        "zero"
    } else {
        "positive"
    }
}

fn main() {
    let x = if 2 > 1 { 10 } else { 20 };
    println("if expression: {x}");

    let y = {
        let a = 5;
        a * 2
    };
    println("block expression: {y}");

    println("classify(-3): {classify(-3)}");
    println("classify(0): {classify(0)}");
    println("classify(7): {classify(7)}");

    let mut i = 0;
    while true {
        i = i + 1;
        if i == 3 {
            break;
        }
    }
    println("loop stopped at {i}");

    let mut evens = 0;
    for n in [1, 2, 3, 4, 5, 6] {
        if n % 2 != 0 {
            continue;
        }
        evens = evens + 1;
    }
    println("even numbers: {evens}");

    assert(i == 3);
    println("assertions passed");
}
