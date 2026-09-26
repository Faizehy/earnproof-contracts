const fs = require('fs');
const path = require('path');

function walk(dir) {
    let results = [];
    const list = fs.readdirSync(dir);
    list.forEach(function(file) {
        file = path.join(dir, file);
        const stat = fs.statSync(file);
        if (stat && stat.isDirectory()) { 
            results = results.concat(walk(file));
        } else { 
            if (file.endsWith('.rs') || file.endsWith('.md')) {
                results.push(file);
            }
        }
    });
    return results;
}

const files = walk('./tests').concat(walk('./docs'));

files.forEach(file => {
    let content = fs.readFileSync(file, 'utf8');
    let changed = false;

    // Special cases for sequences.rs
    if (file.includes('sequences.rs')) {
        if (content.includes('deployment.config.try_set_admin(&next).is_ok()')) {
            content = content.replace('deployment.config.try_set_admin(&next).is_ok()', 
                '{ let r = deployment.config.try_nominate_admin(&next); if r.is_ok() { let _ = deployment.config.try_accept_admin(); } r.is_ok() }');
            changed = true;
        }
    }
    
    // Special cases for pause_matrix.rs
    if (file.includes('pause_matrix.rs')) {
        if (content.includes('settled(d.config.try_set_admin(&next))')) {
            content = content.replace('settled(d.config.try_set_admin(&next))', 
                '{\n                let res = d.config.try_nominate_admin(&next);\n                if res.is_ok() {\n                    let _ = d.config.try_accept_admin();\n                }\n                settled(res)\n            }');
            changed = true;
        }
    }
    
    // ordering.rs Boxed closure
    if (file.includes('ordering.rs')) {
        if (content.includes('|| deployment.config.set_admin(&successor)')) {
            content = content.replace(/\|\| deployment\.config\.set_admin\(&successor\)/g, 
                '|| { deployment.config.nominate_admin(&successor); deployment.config.accept_admin() }');
            changed = true;
        }
    }
    
    // state_machine.rs
    if (file.includes('state_machine.rs')) {
        if (content.includes('client.set_admin(&new_admin);')) {
            content = content.replace(/client\.set_admin\(&new_admin\);/g, 
                '{ client.nominate_admin(&new_admin); client.accept_admin(); }');
            changed = true;
        }
    }
    
    // All other set_admin() replacements
    const setAdminRegex = /([a-zA-Z0-9_\.]+)\.set_admin\(([^)]+)\);/g;
    if (setAdminRegex.test(content)) {
        content = content.replace(setAdminRegex, "$1.nominate_admin($2);\n        $1.accept_admin();");
        changed = true;
    }
    
    // Update try_set_admin in state machine or others if any
    const trySetAdminRegex = /([a-zA-Z0-9_\.]+)\.try_set_admin\(([^)]+)\)/g;
    if (trySetAdminRegex.test(content) && !file.includes('sequences.rs') && !file.includes('pause_matrix.rs')) {
        content = content.replace(trySetAdminRegex, "{ let r = $1.try_nominate_admin($2); if r.is_ok() { let _ = $1.try_accept_admin(); } r }");
        changed = true;
    }

    if (changed) {
        fs.writeFileSync(file, content, 'utf8');
        console.log("Updated: " + file);
    }
});
