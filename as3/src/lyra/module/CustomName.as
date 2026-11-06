package lyra.module {
  import lyra.game.World;
  import lyra.Main;

  public class CustomName extends Module {

    public function CustomName() {
      super("CustomName");
    }

    public static var instance:CustomName = new CustomName();

    public var customName:* = null;

    public var customGuild:* = null;

    public function resetName():void {
      customName = null;
      if (!World.isLoaded()) {
        return;
      }

      var game:* = Main.Game;
      var avatar:* = game.world.myAvatar;
      avatar.objData.strUsername = avatar.pnm;
      avatar.pMC.pAV.objData.strUsername = avatar.pnm;
      avatar.pMC.updateName();
      game.ui.mcPortrait.strName.text = String(avatar.pnm).toUpperCase();
    }

    public function resetGuild():void {
      customGuild = null;
      if (!World.isLoaded()) {
        return;
      }

      var avatar:* = Main.Game.world.myAvatar;
      avatar.pMC.pname.tg.text = avatar.objData.guild == null
        ? ""
        : "< " + String(avatar.objData.guild.Name) + " >";
    }

    override public function onToggle(game:*):void {
      if (!World.isLoaded()) {
        return;
      }

      if (customName !== null) {
        game.world.myAvatar.pMC.pname.ti.text = customName;
        game.ui.mcPortrait.strName.text = String(customName).toUpperCase();
        game.world.myAvatar.objData.strUsername = customName;
        game.world.myAvatar.pMC.pAV.objData.strUsername = customName;
      }

      if (customGuild !== null) {
        game.world.myAvatar.pMC.pname.tg.text = customGuild == ""
          ? ""
          : "< " + String(customGuild) + " >";
      }
    }

    override public function onFrame(game:*):void {
      onToggle(game);
    }
  }

}
