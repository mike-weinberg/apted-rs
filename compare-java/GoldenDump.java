// Differential driver: reads "id<TAB>kind<TAB>t1<TAB>t2<TAB>model" lines on
// stdin and prints, per case, float bits of the distance, the reversed
// distance, the forced left/right path distances, the mapping cost, the exact
// mapping pair list and (trees of at most 6 nodes) the brute-force distance.
import java.io.*;
import java.util.List;
import at.unisalzburg.dbresearch.apted.costmodel.*;
import at.unisalzburg.dbresearch.apted.distance.*;
import at.unisalzburg.dbresearch.apted.node.*;
import at.unisalzburg.dbresearch.apted.parser.BracketStringInputParser;

public class GoldenDump {
  static String hex(float f) { return String.format("%08x", Float.floatToIntBits(f)); }

  static CostModel<StringNodeData> model(String s) {
    if (s.equals("unit")) return new StringUnitCostModel();
    String[] v = s.split(",");
    return new PerEditOperationStringNodeDataCostModel(
        Float.parseFloat(v[0]), Float.parseFloat(v[1]), Float.parseFloat(v[2]));
  }

  public static void main(String[] args) throws Exception {
    BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
    PrintStream out = new PrintStream(new BufferedOutputStream(System.out, 1 << 16), false);
    BracketStringInputParser p = new BracketStringInputParser();
    String line;
    while ((line = in.readLine()) != null) {
      String[] f = line.split("\t");
      Node<StringNodeData> t1 = p.fromString(f[2]);
      Node<StringNodeData> t2 = p.fromString(f[3]);
      boolean big = t1.getNodeCount() > 300 || t2.getNodeCount() > 300;
      boolean small = t1.getNodeCount() <= 6 && t2.getNodeCount() <= 6;
      CostModel<StringNodeData> cm = model(f[4]);
      StringBuilder sb = new StringBuilder();
      APTED<CostModel<StringNodeData>, StringNodeData> a = new APTED<>(cm);
      float d = a.computeEditDistance(t1, t2);
      List<int[]> map = a.computeEditMapping();
      float mc = a.mappingCost(map);
      float drev = new APTED<CostModel<StringNodeData>, StringNodeData>(cm).computeEditDistance(t2, t1);
      String sl = "-", sr = "-";
      if (!big) {
        sl = hex(new APTED<CostModel<StringNodeData>, StringNodeData>(cm).computeEditDistance_spfTest(t1, t2, 0));
        sr = hex(new APTED<CostModel<StringNodeData>, StringNodeData>(cm).computeEditDistance_spfTest(t1, t2, 1));
      }
      sb.append(f[0]).append(" d=").append(hex(d)).append(" drev=").append(hex(drev))
        .append(" spfL=").append(sl).append(" spfR=").append(sr)
        .append(" mc=").append(hex(mc)).append(" map=");
      for (int i = 0; i < map.size(); i++) {
        if (i > 0) sb.append(',');
        sb.append(map.get(i)[0]).append(':').append(map.get(i)[1]);
      }
      if (small) {
        float apm = new AllPossibleMappingsTED<CostModel<StringNodeData>, StringNodeData>(cm).computeEditDistance(t1, t2);
        sb.append(" apm=").append(hex(apm));
      }
      out.println(sb);
    }
    out.flush();
  }
}
