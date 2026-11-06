package lyra.game {
  import lyra.Main;

  [BridgeNamespace("house")]
  public class House {

    [BridgeExport]
    public static function getItems():Array {
      var game:Object = Main.Game;
      return game.world.myAvatar.houseitems;
    }

    [BridgeTsParamType("selector: FlashTypes.InventoryItemSelector")]
    [BridgeExport]
    public static function getItem(selector:Object):Object {
      var game:Object = Main.Game;
      return ItemLookup.find(game.world.myAvatar.houseitems, selector);
    }

    [BridgeExport]
    public static function getSlots():int {
      var game:Object = Main.Game;
      return game.world.myAvatar.objData.iHouseSlots;
    }

    [BridgeExport]
    public static function getUsedSlots():int {
      var game:Object = Main.Game;
      return game.world.myAvatar.houseitems.length;
    }
  }
}
