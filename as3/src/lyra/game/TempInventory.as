package lyra.game {
  import lyra.Main;

  [BridgeNamespace("tempInventory")]
  public class TempInventory {

    [BridgeExport]
    public static function getItems():Array {
      var game:Object = Main.Game;
      return game.world.myAvatar.tempitems;
    }

    [BridgeTsParamType("selector: FlashTypes.InventoryItemSelector")]
    [BridgeExport]
    public static function getItem(selector:Object):Object {
      var game:Object = Main.Game;
      return ItemLookup.find(game.world.myAvatar.tempitems, selector);
    }

    [BridgeTsParamType("selector: FlashTypes.InventoryItemSelector")]
    [BridgeExport]
    public static function contains(selector:Object, quantity:int = 1):Boolean {
      var itemObj:Object = getItem(selector);
      if (!itemObj) {
        return false;
      }

      return itemObj.iQty >= quantity;
    }
  }
}
