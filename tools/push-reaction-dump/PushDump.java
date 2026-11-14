import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.PushReaction;

public final class PushDump {
    private PushDump() {
    }

    private static String array(String name, List<String> names) {
        Collections.sort(names);
        StringBuilder out = new StringBuilder();
        out.append("pub(super) const ").append(name).append(": &[&str] = &[\n");
        for (String entry : names) {
            out.append("    \"minecraft:").append(entry).append("\",\n");
        }
        out.append("];\n\n");
        return out.toString();
    }

    public static void main(String[] args) throws IOException {
        if (args.length != 1) {
            System.err.println("usage: PushDump <output.rs>");
            System.exit(2);
        }
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        List<String> destroy = new ArrayList<>();
        List<String> block = new ArrayList<>();
        List<String> pushOnly = new ArrayList<>();
        List<String> entity = new ArrayList<>();
        List<String> sticky = new ArrayList<>();
        for (Block candidate : BuiltInRegistries.BLOCK) {
            String name = BuiltInRegistries.BLOCK.getKey(candidate).getPath();
            BlockState state = candidate.defaultBlockState();
            PushReaction reaction = state.getPistonPushReaction();
            if (reaction == PushReaction.POPPED) {
                destroy.add(name);
            } else if (reaction == PushReaction.IMMOVEABLE) {
                block.add(name);
            } else if (reaction == PushReaction.PUSH) {
                pushOnly.add(name);
            }
            if (state.getDestroySpeed(EmptyBlockGetter.INSTANCE, BlockPos.ZERO) == -1.0F && reaction != PushReaction.IMMOVEABLE) {
                block.add(name);
            }
            if (state.hasBlockEntity()) {
                entity.add(name);
            }
        }
        StringBuilder out = new StringBuilder();
        out.append(array("DESTROYED", destroy));
        out.append(array("IMMOVABLE", block));
        out.append(array("PUSH_ONLY", pushOnly));
        out.append(array("BLOCK_ENTITIES", entity));
        Path target = Path.of(args[0]).toAbsolutePath();
        Files.createDirectories(target.getParent());
        Files.writeString(target, out.toString(), StandardCharsets.UTF_8);
    }
}
