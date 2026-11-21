# Function catalog extraction

The committed catalog is extracted from `wimi321/lizzieyzy-next` at `cb53acc55eda42c6c671b5f668cd3b597456bf07`, using its shaded jar (SHA-256 `739d77a2531c766375f617673c37f2de4e9760b7a9fc9dbbebde7ee156a4d6ee`). The extraction yields 360 functions in `FunctionCatalog.entries()` order, six categories, 76 sectioned settings targets, and eight maintained bundles. Search results use Java's ranking, including stable-ID ordering when browsing.

The committed file is then edited in place: Java's two interface-style switches become this product's three 外观模式 entries (`menu.appearanceMode*`), and the eleven 详细工具栏 entries (`legacy.*` and `toolbar.detailed`) are left out, as this product leaves out that toolbar. Six entries whose Java dialogs this product folds into 综合设置 get that page as their `section` (Java extracts `null`): `menu.maxTreeWidth`, `menu.initialMaxScoreLead`, `menu.setReplayInterval`, `menu.minPlayoutsForNextMove` and `menu.setCandidatesDelay` get `"section":"engine"`, and `menu.frameFontSize` gets `"section":"theme"`. The English, Korean and Thai aliases of `menu.kataScolreLead` name the corrected 目数显示 items (see `CORRECTIONS` in `tools/extract_java_strings.py`). It holds 350 entries; the bundles keep Java's whole `FunctionSearch.*` vocabulary.

From the repository root, set `JAVA_JAR` to that baseline jar and run with JDK 21:

```sh
javac -cp "$JAVA_JAR" -d /tmp/function-catalog tools/function_catalog/DumpFunctionCatalog.java
java -cp "/tmp/function-catalog:$JAVA_JAR" DumpFunctionCatalog "$JAVA_JAR" crates/lizzie-core/src/function_search/catalog.json
```

The generator reads the real catalog classes and uses `ResourceBundle.Control.getNoFallbackControl(FORMAT_PROPERTIES)`, retaining Java's resource-parent lookup and actual bundle locale. The JSON records the source commit and SHA-256 of the supplied jar. `tools/extract_java_strings.py` consumes these keys when regenerating the UI translations.
