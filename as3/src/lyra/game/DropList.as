package lyra.game {
  import lyra.Main;
  import flash.events.MouseEvent;
  import flash.utils.getQualifiedClassName;

  [BridgeNamespace("drops")]
  public class DropList {
    private static const DROP_COUNT_REGEX:RegExp = /(.*)\s+x\s*(\d*)/i;
    private static const DROP_MC:String = "DFrame2MC";

    private static function getItemId(item:Object):int {
      if (!item || !("ItemID" in item)) {
        return 0;
      }

      return int(item.ItemID);
    }

    private static function isDefaultDropFrame(child:*):Boolean {
      return getQualifiedClassName(child).indexOf(DROP_MC) != -1 && Boolean(child.cnt);
    }

    private static function normalizeDropName(value:*):String {
      return String(value).replace(/^\s+|\s+$/g, "").toLowerCase();
    }

    private static function parseDefaultDropName(value:*):String {
      var name:String = normalizeDropName(value);
      var match:Array = DROP_COUNT_REGEX.exec(name);
      if (match && match.length > 1) {
        return normalizeDropName(match[1]);
      }

      return name;
    }

    private static function getCustomDropEntry(itemId:int):* {
      var source:* = getCustomDropSource();
      if (!source) {
        return null;
      }

      for (var i:int = 0; i < source.numChildren; i++) {
        var child:* = source.getChildAt(i);
        if (child.itemObj && int(child.itemObj.ItemID) == itemId) {
          return child;
        }
      }

      return null;
    }

    private static function getDefaultDropFrame(itemId:int):* {
      var game:Object = Main.Game;
      var item:* = game.world.invTree[itemId];
      if (!item || !item.sName) {
        return null;
      }

      var itemName:String = normalizeDropName(item.sName);
      var children:int = game.ui.dropStack.numChildren;
      for (var i:int = 0; i < children; i++) {
        var child:* = game.ui.dropStack.getChildAt(i);
        if (
          isDefaultDropFrame(child) &&
          parseDefaultDropName(child.cnt.strName.text) == itemName
        ) {
          return child;
        }
      }

      return null;
    }

    [BridgeExport]
    public static function accept(itemId:int):Boolean {
      var game:Object = Main.Game;
      if (isUsingCustomDrops() && !isCustomDropsUiOpen())
        toggleUi();

      // The response handler updates and closes
      // whichever UI is active after the server accepts the item.
      game.sfc.sendXtMessage("zm", "getDrop", [itemId], "str", game.world.curRoom);
      return true;
    }

    [BridgeExport]
    public static function toggleUi():void {
      var game:Object = Main.Game;
      if (isDraggable()) {
        game.cDropsUI.mcDraggable.menuBar.dispatchEvent(new MouseEvent(MouseEvent.CLICK));
      }
      else if (isUsingCustomDrops()) {
        game.cDropsUI.onShow();
      }
    }

    [BridgeExport]
    public static function reject(itemId:int):Boolean {
      if (isUsingCustomDrops()) {
        var entry:* = getCustomDropEntry(itemId);
        if (!entry && !isCustomDropsUiOpen()) {
          toggleUi();
          entry = getCustomDropEntry(itemId);
        }

        if (!entry || !entry.itemObj)
          return false;

        Main.Game.cDropsUI.onBtNo(entry.itemObj);
        return getCustomDropEntry(itemId) == null;
      }

      var frame:* = getDefaultDropFrame(itemId);
      if (!frame)
        return false;

      frame.cnt.nbtn.dispatchEvent(new MouseEvent(MouseEvent.CLICK));
      return !frame.mouseChildren;
    }

    [BridgeExport]
    public static function isUsingCustomDrops():Boolean {
      var game:Object = Main.Game;
      return Boolean(game.cDropsUI) && game.litePreference.data.bCustomDrops;
    }

    [BridgeIgnore]
    public static function isDraggable():Boolean {
      var game:Object = Main.Game;
      return isUsingCustomDrops() && Boolean(game.cDropsUI.mcDraggable);
    }

    private static function getCustomDropSource():* {
      var game:Object = Main.Game;
      if (isDraggable())
        return game.cDropsUI.mcDraggable.menu;
      else if (isUsingCustomDrops())
        return game.cDropsUI;

      return null;
    }

    [BridgeIgnore]
    public static function isCustomDropsUiOpen():Boolean {
      var game:Object = Main.Game;
      if (game.cDropsUI)
        return game.cDropsUI.isMenuOpen();

      return false;
    }
  }
}
