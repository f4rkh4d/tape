# the simplest .tape program: read a name from env, greet it.
let name = env.get("NAME");
if name == "" {
    let name = "stranger";
}
io.write("hello, ");
io.write(name);
io.write("\n");
