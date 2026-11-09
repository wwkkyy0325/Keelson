public final class Main {

    public static void main(String[] args) {
        Engine engine = new Engine();
        System.out.println(engine.rule001(5));
        System.out.println(engine.rule012(40));
        System.out.println(engine.rule030(50));
        System.out.println(engine.rule070(100));
        System.out.println(engine.applied());
    }
}

