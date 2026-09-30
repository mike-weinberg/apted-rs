// Times the upstream Java APTED on tree pairs dumped by
// `cargo run --release --example bench -- --dump DIR`.
// Usage: java -cp <apted classes>:. AptedBench DIR [reps] [warmup]
// Output format matches examples/bench.rs so results can be joined.

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Arrays;
import java.util.List;

import at.unisalzburg.dbresearch.apted.costmodel.StringUnitCostModel;
import at.unisalzburg.dbresearch.apted.distance.APTED;
import at.unisalzburg.dbresearch.apted.node.Node;
import at.unisalzburg.dbresearch.apted.node.StringNodeData;
import at.unisalzburg.dbresearch.apted.parser.BracketStringInputParser;

public class AptedBench {
  static final String[] PAIRS = {
    "random-random", "left-left", "right-right", "binary-binary", "zigzag-zigzag",
    "flat-flat", "left-right", "zigzag-binary", "random-zigzag",
  };

  public static void main(String[] args) throws Exception {
    Path dir = Paths.get(args[0]);
    int reps = args.length > 1 ? Integer.parseInt(args[1]) : 5;
    int warmup = args.length > 2 ? Integer.parseInt(args[2]) : 2;
    BracketStringInputParser parser = new BracketStringInputParser();
    double total = 0;
    for (String name : PAIRS) {
      List<String> lines = Files.readAllLines(dir.resolve(name + ".txt"));
      Node<StringNodeData> t1 = parser.fromString(lines.get(0));
      Node<StringNodeData> t2 = parser.fromString(lines.get(1));
      int n = t1.getNodeCount();
      float d = 0;
      // JIT warm-up runs are not timed.
      for (int i = 0; i < warmup; i++) {
        d = new APTED<StringUnitCostModel, StringNodeData>(new StringUnitCostModel())
            .computeEditDistance(t1, t2);
      }
      double[] times = new double[reps];
      for (int i = 0; i < reps; i++) {
        long start = System.nanoTime();
        APTED<StringUnitCostModel, StringNodeData> apted =
            new APTED<>(new StringUnitCostModel());
        d = apted.computeEditDistance(t1, t2);
        times[i] = (System.nanoTime() - start) / 1e6;
      }
      Arrays.sort(times);
      double median = times[times.length / 2];
      total += median;
      System.out.printf("%-16s n=%-6d d=%-8s median=%9.2f ms%n", name, n, fmt(d), median);
    }
    System.out.printf("%-16s total median=%9.2f ms%n", "ALL", total);
  }

  // Match Rust's float formatting for whole numbers ("1204", not "1204.0").
  static String fmt(float d) {
    return d == Math.rint(d) ? String.valueOf((long) d) : String.valueOf(d);
  }
}
