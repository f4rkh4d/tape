# flip a coin three times with a 100ms pause between flips.
for i in 0..3 {
    let r = random.bits(1);
    let bit = r[0] % 2;
    if bit == 0 {
        io.write("heads\n");
    } else {
        io.write("tails\n");
    }
    time.sleep(100);
}
