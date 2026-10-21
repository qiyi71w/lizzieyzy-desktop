// Java main-window menu bar, in Menu.java add() order: fixed Java
// wimi321/lizzieyzy-next cb53acc55eda42c6c671b5f668cd3b597456bf07, src/main/java/featurecat/lizzie/gui/Menu.java.
// `key` is the DisplayStrings resource key (literal text where Java uses literal text), `id` the
// Java action id (AutoAnalyzeMenu.ACTION_PROPERTY / FunctionCatalog). Which entries are available is
// decided by the command catalog (commands.ts), not here.

export type MenuNode =
  | { t: "sep" }
  | { t: "item" | "check" | "radio" | "dynamic" | "submenu"; key: string; id?: string; when?: string; suffix?: string; items?: MenuNode[] };

export interface TopMenu {
  key: string;
  when?: string;
  items: MenuNode[];
}

export const JAVA_MENUS: TopMenu[] = [
 {
  "key": "Menu.fileMenu",
  "items": [
   {
    "t": "item",
    "key": "Menu.newBoard",
    "id": "files.new"
   },
   {
    "t": "item",
    "key": "Menu.open",
    "id": "files.open"
   },
   {
    "t": "dynamic",
    "key": "Menu.openRecent",
    "id": "files.recent"
   },
   {
    "t": "item",
    "key": "Menu.openUrl",
    "id": "files.url"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.save",
    "id": "files.save"
   },
   {
    "t": "item",
    "key": "Menu.saveAs",
    "id": "files.save-as"
   },
   {
    "t": "submenu",
    "key": "Menu.saveMore",
    "items": [
     {
      "t": "item",
      "key": "Menu.saveRaw",
      "id": "menu.saveRaw"
     },
     {
      "t": "item",
      "key": "Menu.saveCommentRaw",
      "id": "menu.saveCommentRaw"
     },
     {
      "t": "item",
      "key": "Menu.saveBranchRaw",
      "id": "menu.saveBranchRaw"
     },
     {
      "t": "item",
      "key": "Menu.saveMainBoardScreen",
      "id": "menu.saveMainBoardScreen"
     },
     {
      "t": "item",
      "key": "Menu.saveSubBoardScreen",
      "id": "menu.saveSubBoardScreen"
     },
     {
      "t": "item",
      "key": "Menu.saveWinrate",
      "id": "menu.saveWinrate"
     }
    ]
   },
   {
    "t": "item",
    "key": "Menu.saveAndLoad",
    "id": "menu.saveAndLoad"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.copyBoardScreen",
    "id": "menu.copyBoardScreen"
   },
   {
    "t": "item",
    "key": "Menu.copySubBoardScreen",
    "id": "menu.copySubBoardScreen"
   },
   {
    "t": "item",
    "key": "Menu.copySgf",
    "id": "menu.copySgf"
   },
   {
    "t": "item",
    "key": "Menu.pasteSgf",
    "id": "menu.pasteSgf"
   },
   {
    "t": "sep"
   },
   {
    "t": "check",
    "key": "Menu.loadKomi",
    "id": "menu.loadKomi"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.forceExit",
    "id": "menu.forceExit"
   },
   {
    "t": "item",
    "key": "Menu.exit",
    "id": "menu.exit"
   }
  ]
 },
 {
  "key": "Menu.viewMenu",
  "items": [
   {
    "t": "submenu",
    "key": "Menu.panel",
    "items": [
     {
      "t": "check",
      "key": "Menu.subBoard",
      "id": "menu.subboard"
     },
     {
      "t": "check",
      "key": "Menu.winrateGraph",
      "id": "menu.winrateGraph"
     },
     {
      "t": "check",
      "key": "Menu.commitPane",
      "id": "menu.commitPane"
     },
     {
      "t": "check",
      "key": "Menu.variationPane",
      "id": "menu.variationPane"
     },
     {
      "t": "submenu",
      "key": "Menu.variationPaneSettings",
      "items": [
       {
        "t": "check",
        "key": "Menu.showScrollVariation",
        "id": "menu.showScrollVariation"
       },
       {
        "t": "item",
        "key": "Menu.maxTreeWidth",
        "id": "menu.maxTreeWidth"
       },
       {
        "t": "check",
        "key": "Menu.ignoreOutOfWidth",
        "id": "menu.ignoreOutOfWidth"
       }
      ]
     },
     {
      "t": "check",
      "key": "Menu.listPane",
      "id": "menu.listPane"
     },
     {
      "t": "check",
      "key": "Menu.informationPane",
      "id": "menu.informationPane"
     },
     {
      "t": "check",
      "key": "Menu.statusPanel",
      "id": "menu.statusPanel"
     },
     {
      "t": "check",
      "key": "Menu.gtpPanel",
      "id": "menu.gtpPanel"
     },
     {
      "t": "check",
      "key": "Menu.ctrlPanel",
      "id": "menu.ctrlPanel"
     },
     {
      "t": "item",
      "key": "Menu.visualizedPanelSettings",
      "id": "menu.visualizedPanelSettings"
     },
     {
      "t": "sep"
     },
     {
      "t": "check",
      "key": "Menu.independentMainBoard",
      "id": "menu.independentMainBoard"
     },
     {
      "t": "check",
      "key": "Menu.independentMainBoard2",
      "id": "menu.independentMainBoard2"
     },
     {
      "t": "check",
      "key": "Menu.independentSubBoard",
      "id": "menu.independentSubBoard"
     },
     {
      "t": "check",
      "key": "Menu.hawkEye",
      "id": "analysis.bad-moves"
     },
     {
      "t": "check",
      "key": "Menu.SuggestionList",
      "id": "menu.SuggestionList"
     },
     {
      "t": "item",
      "key": "Menu.restoreDefaultPanelSizes",
      "id": "menu.restoreDefaultPanelSizes"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.toolbar",
    "items": [
     {
      "t": "submenu",
      "key": "Menu.topToolBar",
      "id": "menu.topToolBar",
      "items": [
       {
        "t": "check",
        "key": "Menu.topToolBarSeparated"
       },
       {
        "t": "check",
        "key": "Menu.topToolBarCombined"
       },
       {
        "t": "check",
        "key": "Menu.inVisible"
       },
       {
        "t": "sep"
       },
       {
        "t": "check",
        "key": "Menu.autoWrap",
        "id": "menu.autoWrap"
       },
       {
        "t": "submenu",
        "key": "Menu.customToolbar",
        "id": "menu.customTopToolBar",
        "items": [
         {
          "t": "check",
          "key": "Menu.showDoubleMenuBtn"
         },
         {
          "t": "check",
          "key": "Menu.showBasicBtn"
         },
         {
          "t": "check",
          "key": "Menu.playMoveTools"
         },
         {
          "t": "check",
          "key": "Menu.allowAvoidTools"
         },
         {
          "t": "check",
          "key": "Menu.showGameController"
         },
         {
          "t": "check",
          "key": "Menu.showTimeLimitController"
         },
         {
          "t": "check",
          "key": "Menu.showPlayoutLimitController"
         },
         {
          "t": "check",
          "key": "Menu.showAnalyzeController"
         },
         {
          "t": "check",
          "key": "Menu.showCandidateController"
         },
         {
          "t": "check",
          "key": "Menu.showCandidateInfoController"
         },
         {
          "t": "check",
          "key": "Menu.showRuleMenu"
         },
         {
          "t": "check",
          "key": "Menu.showTsumeGoMenu"
         },
         {
          "t": "check",
          "key": "Menu.showParamMenu"
         },
         {
          "t": "check",
          "key": "Menu.showGobanMenu"
         },
         {
          "t": "check",
          "key": "Menu.showSaveLoadMenu"
         }
        ]
       }
      ]
     },
     {
      "t": "submenu",
      "key": "Menu.bottomToolBar",
      "id": "menu.bottomToolBar",
      "items": [
       {
        "t": "check",
        "key": "Menu.visible"
       },
       {
        "t": "check",
        "key": "Menu.inVisible"
       },
       {
        "t": "submenu",
        "key": "详细工具栏(过时的)(仅中文)",
        "when": "chinese",
        "items": [
         {
          "t": "check",
          "key": "使用详细工具栏",
          "id": "toolbar.detailed"
         },
         {
          "t": "check",
          "key": "显示拓展/隐藏详细工具栏按钮"
         },
         {
          "t": "item",
          "key": "设置模块顺序"
         }
        ]
       },
       {
        "t": "sep"
       },
       {
        "t": "submenu",
        "key": "Menu.customToolbar",
        "id": "menu.customToolbarItem",
        "items": [
         {
          "t": "check",
          "key": "BottomToolbar.liveButton"
         },
         {
          "t": "check",
          "key": "BottomToolbar.kataEstimate"
         },
         {
          "t": "check",
          "key": "BottomToolbar.batchOpen"
         },
         {
          "t": "check",
          "key": "BottomToolbar.openfile"
         },
         {
          "t": "check",
          "key": "BottomToolbar.savefile"
         },
         {
          "t": "check",
          "key": "BottomToolbar.finalScore"
         },
         {
          "t": "check",
          "key": "BottomToolbar.heatMap"
         },
         {
          "t": "check",
          "key": "BottomToolbar.badMoves"
         },
         {
          "t": "check",
          "key": "BottomToolbar.analyzeList"
         },
         {
          "t": "check",
          "key": "BottomToolbar.refresh"
         },
         {
          "t": "check",
          "key": "BottomToolbar.analyse"
         },
         {
          "t": "check",
          "key": "BottomToolbar.tryPlay"
         },
         {
          "t": "check",
          "key": "BottomToolbar.setMain"
         },
         {
          "t": "check",
          "key": "BottomToolbar.backMain"
         },
         {
          "t": "check",
          "key": "BottomToolbar.clearButton"
         },
         {
          "t": "check",
          "key": "BottomToolbar.deleteMove"
         },
         {
          "t": "check",
          "key": "BottomToolbar.moveRank"
         },
         {
          "t": "check",
          "key": "BottomToolbar.move"
         },
         {
          "t": "check",
          "key": "BottomToolbar.coords"
         },
         {
          "t": "check",
          "key": "BottomToolbar.autoPlay"
         }
        ]
       }
      ]
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.mainBoardPos",
    "id": "menu.mainBoardPos",
    "items": [
     {
      "t": "item",
      "key": "Menu.leftMove"
     },
     {
      "t": "item",
      "key": "Menu.rightMove"
     }
    ]
   },
   {
    "t": "check",
    "key": "Menu.coordsMenu",
    "id": "menu.coordsMenu"
   },
   {
    "t": "submenu",
    "key": "Menu.moveMenu",
    "id": "menu.moveMenu",
    "items": [
     {
      "t": "check",
      "key": "Menu.noMoveNum"
     },
     {
      "t": "check",
      "key": "Menu.lastOneMoveNum"
     },
     {
      "t": "check",
      "key": "Menu.lastFiveMoveNum"
     },
     {
      "t": "check",
      "key": "Menu.lastTenMoveNum"
     },
     {
      "t": "check",
      "key": "Menu.allMoveNum"
     },
     {
      "t": "check",
      "key": "Menu.anyMoveNum"
     },
     {
      "t": "check",
      "key": "Menu.moveNumberAlwaysFromOne",
      "id": "menu.moveNumberAlwaysFromOne"
     },
     {
      "t": "sep"
     },
     {
      "t": "check",
      "key": "Menu.moveNumberInBracnhFromOne",
      "id": "menu.moveNumberInBranchFromOne"
     },
     {
      "t": "check",
      "key": "Menu.moveNumberInBracnhFromOneContinue"
     },
     {
      "t": "sep"
     },
     {
      "t": "check",
      "key": "Menu.showAllMoveNumberInBranch",
      "id": "menu.showAllMoveNumberInBranch"
     },
     {
      "t": "sep"
     },
     {
      "t": "check",
      "key": "Menu.showMoveNumberOnVariationPane",
      "id": "menu.showMoveNumberOnVariationPane"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.moveRankMenu",
    "id": "menu.moveRankMenu",
    "items": [
     {
      "t": "check",
      "key": "Menu.rankMenu.rankNoneMove"
     },
     {
      "t": "check",
      "key": "Menu.rankMenu.rankLastMove"
     },
     {
      "t": "check",
      "key": "Menu.rankMenu.lblCustomMove"
     },
     {
      "t": "check",
      "key": "Menu.rankMenu.rankAllMove"
     },
     {
      "t": "sep"
     },
     {
      "t": "submenu",
      "key": "Menu.rankMenu.evaluationMode",
      "items": [
       {
        "t": "radio",
        "key": "Menu.rankMenu.evaluationMode.auto"
       },
       {
        "t": "radio",
        "key": "Menu.rankMenu.evaluationMode.score"
       },
       {
        "t": "radio",
        "key": "Menu.rankMenu.evaluationMode.winrate"
       },
       {
        "t": "sep"
       },
       {
        "t": "radio",
        "key": "Menu.rankMenu.evaluationMode.combined"
       }
      ]
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.Suggestions",
    "items": [
     {
      "t": "check",
      "key": "Menu.winrate",
      "id": "menu.suggestion1"
     },
     {
      "t": "check",
      "key": "Menu.visits",
      "id": "menu.suggestion2"
     },
     {
      "t": "check",
      "key": "Menu.scoreLead",
      "id": "menu.suggestion3"
     },
     {
      "t": "sep"
     },
     {
      "t": "check",
      "key": "Menu.showScoreAsDiff",
      "id": "menu.showScoreAsDiff"
     },
     {
      "t": "item",
      "key": "Menu.customInfoOrdr",
      "id": "menu.customInfoOrdr"
     },
     {
      "t": "check",
      "key": "Menu.setCandidatesDelay",
      "id": "menu.setCandidatesDelay"
     },
     {
      "t": "sep"
     },
     {
      "t": "check",
      "key": "Menu.showSuggestionOrder",
      "id": "menu.showSuggestionOrder"
     },
     {
      "t": "check",
      "key": "Menu.showMaxValueReverse",
      "id": "menu.showMaxValueReverse"
     },
     {
      "t": "check",
      "key": "Menu.showWhiteSuggestWhite",
      "id": "menu.showWhiteSuggestWhite"
     },
     {
      "t": "sep"
     },
     {
      "t": "check",
      "key": "Menu.alwaysShowBlackWinrate",
      "id": "settings.black-winrate"
     },
     {
      "t": "check",
      "key": "Menu.showVariationOnMouse",
      "id": "menu.showVariationOnMouse"
     },
     {
      "t": "check",
      "key": "Menu.noRefreshOnMouse",
      "id": "menu.noRefreshOnMouse"
     },
     {
      "t": "item",
      "key": "Menu.setReplayInterval",
      "id": "menu.setReplayInterval"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.nextMoveHint",
    "id": "menu.nextMoveHint",
    "items": [
     {
      "t": "check",
      "key": "Menu.nextMoveHintNone"
     },
     {
      "t": "check",
      "key": "Menu.nextMoveHintSimple"
     },
     {
      "t": "check",
      "key": "Menu.nextMoveHintInformation"
     },
     {
      "t": "item",
      "key": "Menu.minPlayoutsForNextMove",
      "id": "menu.minPlayoutsForNextMove",
      "suffix": "30)"
     }
    ]
   },
   {
    "t": "item",
    "key": "Menu.makeAutoPlay",
    "id": "menu.makeAutoPlay"
   },
   {
    "t": "item",
    "key": "Menu.deletePersistFile",
    "id": "menu.deletePersistFile"
   },
   {
    "t": "sep"
   },
   {
    "t": "submenu",
    "key": "Menu.appearanceTheme",
    "items": [
     {
      "t": "submenu",
      "key": "Menu.boardStyle",
      "id": "menu.boardStyleMenu",
      "items": [
       {
        "t": "check",
        "key": "Menu.boardStyleJapanese"
       },
       {
        "t": "check",
        "key": "Menu.boardStyleChineseClassic"
       }
      ]
     },
     {
      "t": "check",
      "key": "Menu.appleStyleUi",
      "id": "menu.toggleAppleStyle"
     },
     {
      "t": "check",
      "key": "Menu.classicColorTheme",
      "id": "menu.toggleClassicColors"
     },
     {
      "t": "sep"
     },
     {
      "t": "item",
      "key": "Menu.setCustomBoardBackground",
      "id": "menu.setCustomBoard"
     },
     {
      "t": "item",
      "key": "Menu.clearCustomBoardBackground",
      "id": "menu.clearCustomBoard"
     }
    ]
   },
   {
    "t": "sep"
   },
   {
    "t": "submenu",
    "key": "Menu.mainPanelSettings",
    "items": [
     {
      "t": "check",
      "key": "Menu.largeSubBoard",
      "id": "menu.largeSubBoard"
     },
     {
      "t": "check",
      "key": "Menu.largeWinrateGraph",
      "id": "menu.largeWinrateGraph"
     },
     {
      "t": "check",
      "key": "Menu.appendWinrateToComment",
      "id": "menu.appendWinrateToComment"
     },
     {
      "t": "check",
      "key": "Menu.showNameInBoard",
      "id": "menu.showNameInBoard"
     },
     {
      "t": "check",
      "key": "Menu.showCommentConrolPane",
      "id": "menu.showCommentConrolPane"
     },
     {
      "t": "check",
      "key": "Menu.alwaysOnTop",
      "id": "menu.alwaysOnTop"
     },
     {
      "t": "sep"
     },
     {
      "t": "check",
      "key": "Menu.showNewBoardConfirmDialog",
      "id": "menu.showNewBoardConfirmDialog"
     },
     {
      "t": "check",
      "key": "Menu.showReplaceFileConfirmDialog",
      "id": "menu.showReplaceFileConfirmDialog"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.winrateGraphSettings",
    "items": [
     {
      "t": "submenu",
      "key": "Menu.showWinRateOrScoreLeadLine",
      "id": "menu.showWinRateOrScoreLeadLine",
      "items": [
       {
        "t": "check",
        "key": "Menu.showWinRateOrScoreLeadLine.winrate"
       },
       {
        "t": "check",
        "key": "Menu.showWinRateOrScoreLeadLine.scoreLead"
       },
       {
        "t": "check",
        "key": "Menu.showWinRateOrScoreLeadLine.both"
       }
      ]
     },
     {
      "t": "check",
      "key": "Menu.showWinrateGraphFill",
      "id": "menu.showWinrateGraphFill"
     },
     {
      "t": "check",
      "key": "Menu.showBlunderBar",
      "id": "menu.showBlunderBar"
     },
     {
      "t": "check",
      "key": "Menu.showMouseOverWinrateGraph",
      "id": "menu.showMouseOverWinrateGraph"
     },
     {
      "t": "item",
      "key": "Menu.initialMaxScoreLead",
      "id": "menu.initialMaxScoreLead"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.subBoardSettings",
    "items": [
     {
      "t": "submenu",
      "key": "Menu.heatMapSettings",
      "id": "menu.heatMapSettings",
      "items": [
       {
        "t": "check",
        "key": "Menu.showHeat"
       },
       {
        "t": "check",
        "key": "Menu.showHeatAfterCalc"
       },
       {
        "t": "check",
        "key": "Menu.notShowHeat"
       }
      ]
     },
     {
      "t": "check",
      "key": "Menu.subBoardShowVar",
      "id": "menu.subBoardShowVar"
     },
     {
      "t": "check",
      "key": "Menu.subBoardShowRaw"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.kataSettings",
    "items": [
     {
      "t": "submenu",
      "key": "Menu.kataScolreLead",
      "id": "menu.kataScolreLead",
      "items": [
       {
        "t": "check",
        "key": "Menu.leadWithKomi"
       },
       {
        "t": "check",
        "key": "Menu.leadJustScore"
       }
      ]
     },
     {
      "t": "submenu",
      "key": "Menu.kataEstimate",
      "id": "menu.kataEstimate",
      "items": [
       {
        "t": "check",
        "key": "Menu.kataEstimateClose"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateCloseView"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateOnMainBoard"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateOnSubBoard"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateOnBothBoard"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateSaveState",
        "id": "menu.kataEstimateSaveState"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateByTransparentSmall",
        "id": "menu.kataEstimateByTransparentSmall"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateByTransparent"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateByTransparentNotOnLive"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateByBigSquare"
       },
       {
        "t": "check",
        "key": "Menu.kataEstimateBySize"
       },
       {
        "t": "submenu",
        "key": "Menu.kataEstimateInPureNet",
        "id": "menu.kataEstimateInPureNet",
        "items": [
         {
          "t": "check",
          "key": "Menu.kataEstimateByTransparentSmall"
         },
         {
          "t": "check",
          "key": "Menu.kataEstimateByTransparent"
         },
         {
          "t": "check",
          "key": "Menu.kataEstimateByTransparentNotOnLive"
         },
         {
          "t": "check",
          "key": "Menu.kataEstimateByBigSquare"
         },
         {
          "t": "check",
          "key": "Menu.kataEstimateBySize"
         }
        ]
       },
       {
        "t": "check",
        "key": "Menu.useMovesOwnership",
        "id": "menu.useMovesOwnership"
       },
       {
        "t": "check",
        "key": "Menu.useKataEstimateShortcut",
        "id": "menu.useKataEstimateShortcut"
       }
      ]
     }
    ]
   },
   {
    "t": "sep"
   },
   {
    "t": "submenu",
    "key": "Menu.layoutMode",
    "id": "menu.layoutModeMenu",
    "items": [
     {
      "t": "item",
      "key": "Menu.defaultView"
     },
     {
      "t": "item",
      "key": "Menu.classicView"
     },
     {
      "t": "item",
      "key": "Menu.minView"
     },
     {
      "t": "sep"
     },
     {
      "t": "check",
      "key": "Menu.extraMode3"
     },
     {
      "t": "check",
      "key": "Menu.extraMode1"
     },
     {
      "t": "check",
      "key": "Menu.extraMode2"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.independentBoardMode",
    "id": "menu.independentBoardMode",
    "items": [
     {
      "t": "item",
      "key": "Menu.indepBothBoard"
     },
     {
      "t": "item",
      "key": "Menu.indepMainBoard"
     },
     {
      "t": "item",
      "key": "Menu.indepSubBoard"
     },
     {
      "t": "item",
      "key": "Menu.indepExtraBoard"
     }
    ]
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.customLayout",
    "id": "menu.custom-layout-1",
    "suffix": "1{sp}(Alt+8)"
   },
   {
    "t": "item",
    "key": "Menu.customLayout",
    "id": "menu.custom-layout-2",
    "suffix": "2{sp}(Alt+9)"
   }
  ]
 },
 {
  "key": "Menu.game",
  "items": [
   {
    "t": "submenu",
    "key": "Menu.newGame",
    "items": [
     {
      "t": "item",
      "key": "Menu.newGenmoveGame",
      "id": "menu.newGenmoveGame"
     },
     {
      "t": "item",
      "key": "Menu.newAnalyzeModeGame",
      "id": "menu.newAnalyzeModeGame"
     },
     {
      "t": "item",
      "key": "Menu.newEngineGame",
      "id": "menu.newEngineGame"
     },
     {
      "t": "item",
      "key": "Menu.newHumanSlGame",
      "id": "menu.newHumanSlGame"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "Menu.continueGameAgainstAi",
    "id": "menu.continueGameAgainstAi",
    "items": [
     {
      "t": "item",
      "key": "Menu.continueGenmoveGameAsWhite"
     },
     {
      "t": "item",
      "key": "Menu.continueGenmoveGameAsBlack"
     },
     {
      "t": "item",
      "key": "Menu.continueAnalyzeGameAsWhite"
     },
     {
      "t": "item",
      "key": "Menu.continueAnalyzeGameAsBlack"
     }
    ]
   },
   {
    "t": "check",
    "key": "Menu.scoreGame",
    "id": "menu.scoreGame"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.setAiTime",
    "id": "menu.setAiTime"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.breakGame",
    "id": "game.stop-human"
   },
   {
    "t": "item",
    "key": "Menu.breakEngineGame",
    "id": "menu.breakEngineGame"
   },
   {
    "t": "item",
    "key": "Menu.pauseEngineGame",
    "id": "menu.pauseEngineGame"
   },
   {
    "t": "item",
    "key": "Menu.changeEngineGameNumbers",
    "id": "menu.changeEngineGameNumbers"
   },
   {
    "t": "item",
    "key": "Menu.intervention",
    "id": "menu.intervention"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.playBestMove",
    "id": "menu.playBestMove"
   },
   {
    "t": "item",
    "key": "Menu.playPassMove",
    "id": "menu.playPassMove"
   },
   {
    "t": "submenu",
    "key": "Menu.startingPositionSetup",
    "items": [
     {
      "t": "check",
      "key": "Menu.setupModeToggle",
      "id": "menu.setupModeToggle"
     },
     {
      "t": "radio",
      "key": "Menu.setupToolBlack",
      "id": "menu.setupToolBlack"
     },
     {
      "t": "radio",
      "key": "Menu.setupToolWhite",
      "id": "menu.setupToolWhite"
     },
     {
      "t": "radio",
      "key": "Menu.setupToolErase",
      "id": "menu.setupToolErase"
     },
     {
      "t": "sep"
     },
     {
      "t": "item",
      "key": "Menu.setupClearAll",
      "id": "menu.setupClearAll"
     },
     {
      "t": "sep"
     },
     {
      "t": "item",
      "key": "Menu.setupBlackToPlay",
      "id": "menu.setupBlackToPlay"
     },
     {
      "t": "item",
      "key": "Menu.setupWhiteToPlay",
      "id": "menu.setupWhiteToPlay"
     },
     {
      "t": "sep"
     },
     {
      "t": "item",
      "key": "Menu.convertCurrentPositionToStartingPosition",
      "id": "menu.convertCurrentPosition"
     }
    ]
   },
   {
    "t": "item",
    "key": "Menu.game.continueLadder",
    "id": "menu.continueLadder"
   }
  ]
 },
 {
  "key": "Menu.analyze",
  "items": [
   {
    "t": "item",
    "key": "Menu.togglePonder",
    "id": "analysis.toggle"
   },
   {
    "t": "item",
    "key": "Menu.aiCommentary",
    "id": "analysis.commentary"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.hawkEye",
    "id": "analysis.bad-moves"
   },
   {
    "t": "item",
    "key": "Menu.tsumeGo",
    "id": "game.tsumego"
   },
   {
    "t": "item",
    "key": "Menu.captureTsumeGo",
    "id": "menu.captureTsumeGo"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.autoAnalyze",
    "id": "auto-analysis"
   },
   {
    "t": "item",
    "key": "Menu.batchAnalyze",
    "id": "batch-analysis"
   },
   {
    "t": "item",
    "key": "Menu.batchAnalysisMode",
    "id": "batch-deep-analysis"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.stopAutoAnalyze",
    "id": "stop-analysis"
   },
   {
    "t": "item",
    "key": "Menu.batchAnalyzeTable",
    "id": "batch-progress"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.wholeGameDeepAnalysis",
    "id": "whole-game-deep-analysis"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.flashAnalyzeAllGame",
    "id": "whole-game-lightning-overview"
   },
   {
    "t": "item",
    "key": "Menu.flashAnalyzePartGame",
    "id": "partial-lightning-analysis"
   },
   {
    "t": "item",
    "key": "Menu.flashAnalyzeAllBranches",
    "id": "all-branches-lightning-analysis"
   },
   {
    "t": "item",
    "key": "Menu.flashAnalyzeSettings",
    "id": "lightning-analysis-settings"
   },
   {
    "t": "item",
    "key": "Menu.playerStrengthEstimate",
    "id": "menu.playerStrengthEstimate"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.showHeatmap",
    "id": "menu.showHeatmap"
   },
   {
    "t": "check",
    "key": "Menu.showPolicy",
    "id": "menu.showPolicy"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.clearAllLizzieCache",
    "id": "menu.clearAllLizzieCache"
   },
   {
    "t": "item",
    "key": "Menu.clearThisLizzieCache",
    "id": "menu.clearThisLizzieCache"
   },
   {
    "t": "item",
    "key": "Menu.clearAllLizzieBestmoves",
    "id": "menu.clearAllLizzieBestmoves"
   },
   {
    "t": "item",
    "key": "Menu.clearThisLizzieBestmoves",
    "id": "menu.clearThisLizzieBestmoves"
   }
  ]
 },
 {
  "key": "Menu.edit",
  "items": [
   {
    "t": "item",
    "key": "Menu.addBlack",
    "id": "menu.addBlack"
   },
   {
    "t": "item",
    "key": "Menu.addWhite",
    "id": "menu.addWhite"
   },
   {
    "t": "item",
    "key": "Menu.alternatelyMoves",
    "id": "menu.alternatelyMoves"
   },
   {
    "t": "check",
    "key": "Menu.allowDoubleClick",
    "id": "menu.allowDoubleClick"
   },
   {
    "t": "check",
    "key": "Menu.allowDrag",
    "id": "menu.allowDrag"
   },
   {
    "t": "check",
    "key": "Menu.allowClickReview",
    "id": "menu.allowClickReview"
   },
   {
    "t": "sep"
   },
   {
    "t": "check",
    "key": "Menu.insertBlack",
    "id": "menu.insertBlack"
   },
   {
    "t": "check",
    "key": "Menu.insertWhite",
    "id": "menu.insertWhite"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.clearBoard",
    "id": "menu.clearBoard"
   },
   {
    "t": "item",
    "key": "Menu.backToMainBranch",
    "id": "menu.backToMainBranch"
   },
   {
    "t": "item",
    "key": "Menu.setAsMain",
    "id": "menu.setAsMain"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.jumpToFirst",
    "id": "menu.jumpToFirst"
   },
   {
    "t": "item",
    "key": "Menu.jumpToLast",
    "id": "menu.jumpToLast"
   },
   {
    "t": "item",
    "key": "Menu.jumpToLeft",
    "id": "menu.jumpToLeft"
   },
   {
    "t": "item",
    "key": "Menu.jumpToRight",
    "id": "menu.jumpToRight"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.delete",
    "id": "menu.delete"
   },
   {
    "t": "item",
    "key": "Menu.deleteBranch",
    "id": "menu.deleteBranch"
   },
   {
    "t": "item",
    "key": "Menu.undoDelete",
    "id": "menu.undoDelete"
   },
   {
    "t": "item",
    "key": "Menu.redoDelete",
    "id": "menu.redoDelete"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.setInfo",
    "id": "game.komi"
   },
   {
    "t": "item",
    "key": "Menu.setBoard",
    "id": "menu.setBoard"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.exchange",
    "id": "menu.exchange"
   },
   {
    "t": "item",
    "key": "Menu.spinRight",
    "id": "menu.spinRight"
   },
   {
    "t": "item",
    "key": "Menu.spinLeft",
    "id": "menu.spinLeft"
   },
   {
    "t": "item",
    "key": "Menu.mirrorVertical",
    "id": "menu.mirrorVertical"
   },
   {
    "t": "item",
    "key": "Menu.mirrorHorizon",
    "id": "menu.mirrorHorizon"
   }
  ]
 },
 {
  "key": "Menu.sync",
  "items": [
   {
    "t": "item",
    "key": "Menu.yikeLive",
    "id": "menu.yikeLive"
   },
   {
    "t": "item",
    "key": "Menu.yikeLiveWeb",
    "id": "menu.yikeLiveWeb"
   },
   {
    "t": "item",
    "key": "Menu.yikeRoom",
    "id": "menu.yikeRoom"
   },
   {
    "t": "item",
    "key": "Fox.title"
   },
   {
    "t": "item",
    "key": "Menu.foxKifu",
    "id": "menu.foxKifu"
   },
   {
    "t": "item",
    "key": "Menu.tencentKifu",
    "id": "menu.tencentKifu"
   },
   {
    "t": "item",
    "key": "Menu.readBoard",
    "id": "sync.board",
    "when": "windows"
   },
   {
    "t": "sep"
   },
   {
    "t": "submenu",
    "key": "Menu.webBoard",
    "items": [
     {
      "t": "item",
      "key": "Menu.webBoardStart",
      "id": "menu.webBoardToggle"
     },
     {
      "t": "item",
      "key": "Menu.webBoardCopyUrl",
      "id": "menu.webBoardCopyUrl"
     }
    ]
   },
   {
    "t": "sep"
   },
   {
    "t": "check",
    "key": "Menu.EnableEnterYikeGame",
    "id": "menu.EnableEnterYikeGame"
   },
   {
    "t": "check",
    "key": "Menu.alwaysGoToLastMove",
    "id": "menu.alwaysGoToLastMove"
   },
   {
    "t": "submenu",
    "key": "Menu.readBoardSettings",
    "id": "sync.settings",
    "items": [
     {
      "t": "check",
      "key": "Menu.alwaysKeepBoardStatSync",
      "id": "menu.alwaysKeepBoardStatSync"
     },
     {
      "t": "check",
      "key": "Menu.readBoardGetFocus",
      "id": "menu.readBoardGetFocus"
     }
    ]
   }
  ]
 },
 {
  "key": "Menu.help",
  "items": [
   {
    "t": "item",
    "key": "Menu.diagnosticsAndLogs",
    "id": "menu.diagnosticsAndLogs"
   },
   {
    "t": "item",
    "key": "Menu.stopFullTrace",
    "id": "menu.stopFullTrace"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.about",
    "id": "menu.about"
   },
   {
    "t": "item",
    "key": "Menu.checkUpdate",
    "id": "menu.checkUpdate"
   },
   {
    "t": "sep"
   },
   // Product entry (no Java counterpart): import of LizzieYzy Next settings, next to the other
   // user-data operation of this menu.
   {
    "t": "item",
    "key": "product.javaImport",
    "id": "product.javaImport"
   },
   {
    "t": "item",
    "key": "Menu.clearAllPersonalData",
    "id": "menu.clearUserData"
   }
  ]
 },
 {
  "key": "Menu.quickLinks",
  "items": [
   {
    "t": "dynamic",
    "key": "quickLinks.programs",
    "id": "tools.quick-start",
    "when": "quickLinks"
   },
   {
    "t": "item",
    "key": "Menu.editProgram",
    "when": "quickLinks"
   }
  ],
  "when": "quickLinks"
 },
 {
  "key": "Menu.settings",
  "items": [
   {
    "t": "item",
    "key": "Menu.engineConfig",
    "id": "engine.configuration"
   },
   {
    "t": "item",
    "key": "Menu.remoteCompute",
    "id": "engine.remote"
   },
   {
    "t": "item",
    "key": "Menu.engineRules",
    "id": "engine.rules"
   },
   {
    "t": "item",
    "key": "Menu.engineParameters",
    "id": "engine.parameters"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.initSettings",
    "id": "menu.initSettings"
   },
   {
    "t": "item",
    "key": "Menu.autoSetup",
    "id": "engine.setup"
   },
   {
    "t": "item",
    "key": "Menu.comprehensiveSettings",
    "id": "settings.general"
   },
   {
    "t": "item",
    "key": "Menu.theme",
    "id": "menu.theme"
   },
   {
    "t": "sep"
   },
   {
    "t": "submenu",
    "key": "menu.language",
    "id": "menu.language",
    "items": [
     {
      "t": "check",
      "key": "menu.language.default"
     },
     {
      "t": "check",
      "key": "简体中文"
     },
     {
      "t": "check",
      "key": "繁體中文"
     },
     {
      "t": "check",
      "key": "English"
     },
     {
      "t": "check",
      "key": "日本語"
     },
     {
      "t": "check",
      "key": "한국어"
     },
     {
      "t": "check",
      "key": "ไทย"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "menu.frameFontSize",
    "id": "menu.frameFontSize",
    "items": [
     {
      "t": "check",
      "key": "menu.frameFontSizeSmall"
     },
     {
      "t": "check",
      "key": "menu.frameFontSizeMiddle"
     },
     {
      "t": "check",
      "key": "menu.frameFontSizeBig"
     },
     {
      "t": "check",
      "key": "menu.frameFontSizeOther"
     }
    ]
   },
   {
    "t": "submenu",
    "key": "menu.frameLooks",
    "id": "menu.frameLooks",
    "items": [
     {
      "t": "check",
      "key": "menu.frameLooksSystem"
     },
     {
      "t": "check",
      "key": "menu.frameLooksJava"
     }
    ]
   },
   {
    "t": "sep"
   },
   {
    "t": "check",
    "key": "Menu.playSound",
    "id": "menu.playSound"
   },
   {
    "t": "check",
    "key": "Menu.notPlaySoundInSync",
    "id": "menu.notPlaySoundInSync"
   },
   {
    "t": "check",
    "key": "Menu.showContribute",
    "id": "menu.showContribute"
   }
  ]
 },
 {
  "key": "Menu.contributeMenu",
  "items": [
   {
    "t": "item",
    "key": "Menu.kataGoDistributedTraining",
    "id": "menu.kataGoDistributedTraining",
    "when": "contribute"
   },
   {
    "t": "item",
    "key": "Menu.kataGoTrainingSettings",
    "id": "menu.kataGoTrainingSettings",
    "when": "contribute"
   },
   {
    "t": "item",
    "key": "Menu.kataGoOfficialWebsite",
    "id": "menu.kataGoOfficialWebsite",
    "when": "contribute"
   }
  ],
  "when": "contribute"
 },
 {
  "key": "Menu.noEngine",
  "items": [
   {
    "t": "dynamic",
    "key": "engine.list",
    "id": "engine.select"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.restartCurrentEngine",
    "id": "menu.restartCurrentEngine"
   },
   {
    "t": "submenu",
    "key": "Menu.shutdownEngine",
    "items": [
     {
      "t": "item",
      "key": "Menu.shutdownCurrentEngine",
      "id": "menu.shutdownCurrentEngine"
     },
     {
      "t": "item",
      "key": "Menu.shutdownOtherEngine",
      "id": "menu.shutdownOtherEngine"
     },
     {
      "t": "item",
      "key": "Menu.shutdownAllEngine",
      "id": "menu.shutdownAllEngine"
     }
    ]
   }
  ]
 },
 {
  "key": "Menu.noEngine",
  "items": [
   {
    "t": "dynamic",
    "key": "engine.list",
    "id": "engine.select-secondary"
   },
   {
    "t": "sep"
   },
   {
    "t": "item",
    "key": "Menu.restartCurrentEngine",
    "id": "menu.restartCurrentEngine2"
   },
   {
    "t": "item",
    "key": "Menu.shutdownCurrentEngine",
    "id": "menu.shutdownCurrentEngine2"
   }
  ],
  "when": "doubleEngine"
 },
 {
  "key": "Menu.aiCoach",
  "items": [
   {
    "t": "item",
    "key": "Menu.aiCoach",
    "id": "game.ai-coach"
   }
  ]
 },
 {
  "key": "Menu.aiCommentary",
  "items": [
   {
    "t": "item",
    "key": "Menu.aiCommentary",
    "id": "analysis.commentary"
   }
  ]
 }
];
