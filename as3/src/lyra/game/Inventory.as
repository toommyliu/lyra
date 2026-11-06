package lyra.game {
  import lyra.Main;

  [BridgeNamespace("inventory")]
  public class Inventory {

    [BridgeExport]
    public static function getItems():Array {
      var game:Object = Main.Game;
      return game.world.myAvatar.items;
    }

    [BridgeTsParamType("selector: FlashTypes.InventoryItemSelector")]
    [BridgeExport]
    public static function getItem(selector:Object):Object {
      var game:Object = Main.Game;
      return ItemLookup.find(game.world.myAvatar.items, selector);
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

    [BridgeExport]
    public static function getSlots():int {
      var game:Object = Main.Game;
      return game.world.myAvatar.objData.iBagSlots;
    }

    [BridgeExport]
    public static function getUsedSlots():int {
      var game:Object = Main.Game;
      var categories:Class = Class(Main.getInstance().getGameDomain().getDefinition("InvCat"));
      return categories["countBag"](game.world.myAvatar.items);
    }

    [BridgeExport]
    public static function getMiscSlots():int {
      var categories:Class = Class(Main.getInstance().getGameDomain().getDefinition("InvCat"));
      return categories["MISC_SLOTS"];
    }

    [BridgeTsParamType("selector: FlashTypes.InventoryItemSelector")]
    [BridgeExport]
    public static function equip(selector:Object):Boolean {
      var itemObj:Object = getItem(selector);
      if (!itemObj) {
        return false;
      }

      var game:Object = Main.Game;
      if (itemObj.sType == "Item") {
        if (!itemObj.ItemID || !itemObj.sDesc || !itemObj.sFile || !itemObj.sName) {
          return false;
        }

        game.world.equipUseableItem(itemObj);
        return true;
      }

      return game.world.sendEquipItemRequest({ItemID: itemObj.ItemID});
    }

    [BridgeTsParamType("selector: FlashTypes.InventoryItemSelector")]
    [BridgeExport("use")]
    public static function useItem(selector:Object):Boolean {
      var itemObj:Object = getItem(selector);
      if (!itemObj) {
        return false;
      }

      var category:String = String(itemObj.sType).toLowerCase();
      var link:String = String(itemObj.sLink).toLowerCase();
      if (category != "serveruse" && link != "elixir" && link != "tonic") {
        return false;
      }

      var game:Object = Main.Game;
      game.world.sendUseItemRequest(itemObj);
      return true;
    }

    [BridgeTsParamType("selector: FlashTypes.InventoryItemSelector")]
    [BridgeExport]
    public static function wear(selector:Object):Boolean {
      var itemObj:Object = getItem(selector);
      if (!itemObj || !itemObj.hasOwnProperty("bWear")) {
        return false;
      }

      if (itemObj.bWear == 1) {
        return true;
      }

      var game:Object = Main.Game;
      if (!game.world.coolDown("wearItem")) {
        return false;
      }

      game.sfc.sendXtMessage(
        "zm",
        "wearItem",
        [itemObj.ItemID],
        "str",
        game.world.curRoom
      );
      return true;
    }

    [BridgeTsParamType("selector: FlashTypes.InventoryItemSelector")]
    [BridgeExport]
    public static function unequipConsumable(selector:Object):Boolean {
      var itemObj:Object = getItem(selector);
      if (!itemObj || itemObj.sType != "Item") {
        return false;
      }

      if (itemObj.bEquip != 1) {
        return true;
      }

      var game:Object = Main.Game;
      game.world.unequipUseableItem(itemObj);
      return true;
    }
  }
}
