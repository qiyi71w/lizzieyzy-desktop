import featurecat.lizzie.search.FunctionCatalog;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.ResourceBundle;
import java.util.TreeMap;
import org.json.JSONObject;

/** Reads the real fixed-baseline classes, not a second hand-maintained catalog. */
public final class DumpFunctionCatalog {
  private static final String SHA = "cb53acc55eda42c6c671b5f668cd3b597456bf07";

  public static void main(String[] args) throws Exception {
    if (args.length != 2) throw new IllegalArgumentException("<baseline.jar> <output.json>");
    var entries = new ArrayList<Map<String, Object>>();
    var keys = new LinkedHashSet<String>();
    int settings = 0;
    for (var entry : FunctionCatalog.entries()) {
      var row = new LinkedHashMap<String, Object>();
      row.put("id", entry.id());
      row.put("title", entry.titleKey());
      row.put("description", entry.descriptionKey());
      row.put("category", entry.categoryKey());
      String target = entry.targetType().name().toLowerCase(Locale.ROOT);
      row.put("target", Character.toUpperCase(target.charAt(0)) + target.substring(1));
      row.put("path", entry.pathKeys());
      row.put("shortcut", entry.shortcut());
      row.put("aliases", entry.strongAliasesKey());
      row.put("weakAliases", entry.weakAliasesKey());
      var setting = FunctionCatalog.configSettingTarget(entry.id());
      row.put("section", setting == null ? JSONObject.NULL : setting.section().name().toLowerCase(Locale.ROOT));
      if (setting != null) settings++;
      entries.add(row);
      keys.addAll(List.of(entry.titleKey(), entry.descriptionKey(), entry.categoryKey(),
          entry.strongAliasesKey(), entry.weakAliasesKey()));
      keys.addAll(entry.pathKeys());
    }
    keys.remove("");
    keys.addAll(FunctionCatalog.categoryKeys());
    if (entries.size() != 360 || settings != 76 || FunctionCatalog.categoryKeys().size() != 6)
      throw new IllegalStateException("Unexpected catalog: " + entries.size() + " entries, " + settings + " settings");
    var bundles = new ArrayList<Map<String, Object>>();
    var locales = List.of(Locale.ROOT, Locale.US, Locale.CHINA, Locale.TAIWAN,
        Locale.forLanguageTag("zh-HK"), Locale.JAPAN, Locale.KOREA, Locale.forLanguageTag("th-TH"));
    var control = ResourceBundle.Control.getNoFallbackControl(ResourceBundle.Control.FORMAT_PROPERTIES);
    for (var locale : locales) {
      var bundle = ResourceBundle.getBundle("l10n.DisplayStrings", locale,
          DumpFunctionCatalog.class.getClassLoader(), control);
      var text = new TreeMap<String, String>();
      for (var key : keys) {
        if (bundle.containsKey(key)) text.put(key, bundle.getString(key));
      }
      // Include dialog keys too, so consumers can extract the complete search vocabulary.
      for (var key : bundle.keySet()) {
        if (key.startsWith("FunctionSearch.")) text.put(key, bundle.getString(key));
      }
      bundles.add(Map.of("locale", bundle.getLocale().toString(), "strings", text));
    }
    var result = new LinkedHashMap<String, Object>();
    result.put("source", "wimi321/lizzieyzy-next");
    result.put("javaSha", SHA);
    result.put("jarSha256", HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(Path.of(args[0])))));
    result.put("categories", FunctionCatalog.categoryKeys());
    Files.createDirectories(Path.of(args[1]).toAbsolutePath().getParent());
    String header = new JSONObject(result).toString();
    var json = new StringBuilder(header.substring(0, header.length() - 1));
    appendRows(json, "entries", entries);
    appendRows(json, "bundles", bundles);
    Files.writeString(Path.of(args[1]), json.append("\n}\n").toString());
    System.out.println(entries.size() + " entries, " + settings + " sectioned targets, " + bundles.size() + " bundles");
  }

  private static void appendRows(StringBuilder json, String key, List<Map<String, Object>> rows) {
    json.append(",\n").append(JSONObject.quote(key)).append(":[\n");
    for (int i = 0; i < rows.size(); i++) {
      if (i > 0) json.append(",\n");
      json.append(new JSONObject(rows.get(i)).toString());
    }
    json.append("\n]");
  }
}
